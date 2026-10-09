// MERGE built from JavaScript against a live server, in either runtime: its
// three kinds of arm, their order, RETURNING with merge_action(), MERGE as a
// common table expression, and the typestate that keeps a MERGE without an
// arm, or an arm with an action its row cannot take, from being built.

import assert from "node:assert/strict";
import { after, before, beforeEach, test } from "node:test";

import {
  col,
  ConstructionError,
  insert,
  merge,
  MergeAction,
  Pool,
  ReturningRow,
  select,
  Table,
  With,
} from "../lib/index.js";
import { scratchDatabase } from "./support.ts";

let database: { dsn: string; drop(): Promise<void> } | undefined;
let shared: Pool | undefined;

function pool(): Pool {
  if (!shared) throw new Error("the scratch database was not created");
  return shared;
}

const target = new Table("account", { schema: "app", alias: "t" });
const source = new Table("staged", { alias: "s" });
const on = target.col("id").eq(source.col("id"));

before(async () => {
  database = await scratchDatabase("pgorm_napi_merge");
  shared = new Pool(database.dsn, { maxSize: 4 });
  for (
    const sql of [
      "CREATE SCHEMA app",
      "CREATE TABLE app.account (id int8 PRIMARY KEY, name text, visits int4 NOT NULL DEFAULT 0)",
      "CREATE TABLE staged (id int8, name text, kind text)",
      "CREATE TABLE app.archived () INHERITS (app.account)",
      "CREATE TABLE ticket (id int8 GENERATED ALWAYS AS IDENTITY PRIMARY KEY, note text)",
    ]
  ) {
    await pool().execute(sql);
  }
});

beforeEach(async () => {
  await pool().execute("TRUNCATE app.account, app.archived, staged, ticket");
  await pool().execute("INSERT INTO app.account VALUES (1, 'Alice', 0), (2, 'Bob', 0), (3, 'Carol', 0)");
  await pool().execute("INSERT INTO staged VALUES (1, 'Alicia', 'rename'), (2, NULL, 'drop'), (4, 'Dan', 'add')");
});

after(async () => {
  await shared?.close();
  await database?.drop();
});

function refused(pattern: RegExp): (error: unknown) => true {
  return (error: unknown) => {
    assert.ok(error instanceof ConstructionError, `expected a ConstructionError, got ${error}`);
    assert.match(error.message, pattern);
    return true;
  };
}

async function accounts(): Promise<unknown[]> {
  const rows = await pool().query("SELECT id, name, visits FROM ONLY app.account ORDER BY id");
  return rows.map((row) => [row.id, row.name, row.visits]);
}

// [spec:pgorm:req:napi.merge/test]
test("a MERGE updates, deletes and inserts by the arm each row takes, and returns each row's action", async () => {
  const sync = merge(target, source, on)
    .whenMatched(MergeAction.update("name", source.col("name")).set("visits", target.col("visits").add(1)))
    .whenMatched(MergeAction.delete(), { condition: source.col("name").isNull() })
    .whenNotMatched(MergeAction.insert("id", source.col("id")).set("name", source.col("name")))
    .whenNotMatchedBySource(MergeAction.update("name", "orphan"))
    .returningAction()
    .returning([target.col("id"), ReturningRow.old.col("name").as("was")]);
  const rows = await pool().query(sync);
  assert.deepStrictEqual(
    rows.map((row) => [row.merge_action, row.id, row.was]).sort((a, b) => Number(a[1]) - Number(b[1])),
    [["UPDATE", 1n, "Alice"], ["DELETE", 2n, "Bob"], ["UPDATE", 3n, "Carol"], ["INSERT", 4n, null]],
  );
  assert.deepStrictEqual(await accounts(), [[1n, "Alicia", 1], [3n, "orphan", 0], [4n, "Dan", 0]]);
});

// [spec:pgorm:req:napi.merge/test]
test("a row takes the first conditional arm that holds, and otherwise its kind's unconditional arm", async () => {
  const sync = merge(target, source, on)
    .whenMatched(MergeAction.update("name", "first"))
    .whenMatched(MergeAction.update("name", "renamed"), { condition: source.col("kind").eq("rename") })
    .whenMatched(MergeAction.doNothing(), { condition: source.col("kind").eq("rename") })
    .whenMatched(MergeAction.update("name", "fallback"))
    .whenNotMatched(MergeAction.doNothing(), { condition: source.col("kind").eq("add") })
    .whenNotMatched(MergeAction.insertDefaults());
  assert.equal(await pool().execute(sync), 2);
  assert.deepStrictEqual(await accounts(), [[1n, "renamed", 0], [2n, "fallback", 0], [3n, "Carol", 0]]);
});

// [spec:pgorm:req:napi.merge/test]
test("a MERGE with no WHEN arm has nothing to inspect, and every terminal refuses it before sending it", async () => {
  const pending = merge(target, source, on);
  assert.equal("inspect" in pending, false);
  for (const run of [
    () => pool().execute(pending as never),
    () => pool().query(pending as never),
    () => pool().one(pending as never),
    () => pool().optional(pending as never),
  ]) {
    await assert.rejects(run(), refused(/MERGE needs a WHEN arm/));
  }
  await assert.rejects(pool().stream(pending as never).next(), refused(/MERGE needs a WHEN arm/));
  assert.deepStrictEqual(await accounts(), [[1n, "Alice", 0], [2n, "Bob", 0], [3n, "Carol", 0]]);
});

// [spec:pgorm:req:napi.merge/test]
test("an arm takes only an action its kind of row can take", () => {
  const pending = merge(target, source, on);
  const statement = pending.whenMatched(MergeAction.delete());
  for (const built of [pending, statement]) {
    assert.throws(() => built.whenNotMatched(MergeAction.update("name", "x") as never), refused(/NOT MATCHED arm inserts or does nothing, not an update/));
    assert.throws(() => built.whenNotMatched(MergeAction.delete() as never), refused(/not a delete/));
    assert.throws(() => built.whenMatched(MergeAction.insert("id", 1) as never), refused(/target row updates, deletes or does nothing, not an insert/));
    assert.throws(() => built.whenMatched(MergeAction.insertDefaults() as never), refused(/not an insert of defaults/));
    assert.throws(() => built.whenNotMatchedBySource(MergeAction.insert("id", 1) as never), refused(/not an insert/));
    assert.throws(() => built.whenMatched(col("x") as never), refused(/takes a MergeAction/));
  }
  assert.throws(() => MergeAction.update("name", null as never), refused(/null has no kind/));
  assert.throws(() => MergeAction.insert("id", 1).overriding("never" as never), refused(/overriding/));
  assert.throws(() => merge(col("t") as never, source, on), refused(/target is a Table/));
  assert.throws(
    () => statement.with(With.recursive("r", select(col("n")).from(new Table("r")))),
    refused(/no WITH RECURSIVE before a MERGE/),
  );
});

// [spec:pgorm:req:napi.merge/test]
test("a MERGE with a RETURNING list is a common table expression's body, and reads one", async () => {
  const changed = merge(target, source, on)
    .whenMatched(MergeAction.update("name", source.col("name")), { condition: source.col("name").isNotNull() })
    .whenNotMatched(MergeAction.insert("id", source.col("id")).set("name", source.col("name")))
    .returningAction()
    .returning([target.col("id")]);
  const rows = await pool().query(
    select(col("merge_action"), col("id")).from(new Table("changed")).orderBy(col("id").asc()).with(new With("changed", changed)),
  );
  assert.deepStrictEqual(rows.map((row) => [row.merge_action, row.id]), [["UPDATE", 1n], ["INSERT", 4n]]);
  const fresh = new Table("fresh");
  const fromCte = merge(target, fresh, target.col("id").eq(fresh.col("id")))
    .whenNotMatched(MergeAction.insert("id", fresh.col("id")))
    .with(new With("fresh", select(col("id").add(100).as("id")).from(new Table("staged"))));
  assert.equal(await pool().execute(fromCte), 3);
});

// [spec:pgorm:req:napi.merge/test]
test("ONLY leaves inheriting tables alone, and OVERRIDING writes an identity column", async () => {
  await pool().execute("INSERT INTO app.archived VALUES (1, 'Archived', 0)");
  const cleared = merge(target, source, on).whenMatched(MergeAction.delete()).only();
  assert.equal(await pool().execute(cleared), 2);
  assert.deepStrictEqual(
    (await pool().query("SELECT id, name FROM app.archived")).map((row) => [row.id, row.name]),
    [[1n, "Archived"]],
  );
  const ticket = new Table("ticket");
  const staged = new Table("staged");
  const tickets = merge(ticket, staged, ticket.col("id").eq(staged.col("id")))
    .whenNotMatched(MergeAction.insert("id", staged.col("id")).set("note", staged.col("name")).overriding("systemValue"))
    .returning([ticket.col("id")]);
  assert.deepStrictEqual((await pool().query(tickets)).map((row) => row.id).sort(), [1n, 2n, 4n]);
  await pool().execute(insert(new Table("staged")).columns("id").values(9));
});

// [spec:pgorm:req:napi.merge/test]
test("a subquery is a MERGE's source", async () => {
  const renames = select(col("id"), col("name")).from(new Table("staged")).where(col("kind").eq("rename")).as("s");
  assert.equal(
    await pool().execute(merge(target, renames, target.col("id").eq(renames.col("id"))).whenMatched(MergeAction.update("name", renames.col("name")))),
    1,
  );
  assert.deepStrictEqual(await accounts(), [[1n, "Alicia", 0], [2n, "Bob", 0], [3n, "Carol", 0]]);
});
