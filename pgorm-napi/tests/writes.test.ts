// INSERT, UPDATE and DELETE built from JavaScript against a live server, in
// either runtime: rows, conflicts, RETURNING row versions, writes as common
// table expressions, and the refusals that keep a write that cannot run from
// being sent.

import assert from "node:assert/strict";
import { after, before, beforeEach, test } from "node:test";

import {
  call,
  col,
  Conflict,
  ConstructionError,
  DatabaseError,
  deleteFrom,
  insert,
  Pool,
  ReturningRow,
  select,
  Table,
  TypeName,
  update,
  Value,
  With,
} from "../lib/index.js";
import { scratchDatabase } from "./support.ts";

let database: { dsn: string; drop(): Promise<void> } | undefined;
let shared: Pool | undefined;

function pool(): Pool {
  if (!shared) throw new Error("the scratch database was not created");
  return shared;
}

const account = new Table("account", { schema: "app" });
const staged = new Table("staged");
const mood = new TypeName("mood", { schema: "app" });

before(async () => {
  database = await scratchDatabase("pgorm_napi_writes");
  shared = new Pool(database.dsn, { maxSize: 4 });
  for (
    const sql of [
      "CREATE SCHEMA app",
      "CREATE TYPE app.mood AS ENUM ('calm', 'glad')",
      `CREATE TABLE app.account (
         id int8 PRIMARY KEY, name text NOT NULL, visits int4 NOT NULL DEFAULT 0,
         mood app.mood, email text, active bool NOT NULL DEFAULT true,
         CONSTRAINT account_email_key UNIQUE (email)
       )`,
      "CREATE UNIQUE INDEX account_active_name ON app.account (lower(name)) WHERE active",
      "CREATE TABLE staged (id int8, name text)",
      "CREATE TABLE ticket (id int8 GENERATED ALWAYS AS IDENTITY PRIMARY KEY, note text NOT NULL DEFAULT 'none')",
    ]
  ) {
    await pool().execute(sql);
  }
});

beforeEach(async () => {
  await pool().execute("TRUNCATE app.account, staged, ticket RESTART IDENTITY");
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

async function names(): Promise<unknown[]> {
  const rows = await pool().query(select(col("id"), col("name")).from(account).orderBy(col("id").asc()));
  return rows.map((row) => [row.id, row.name]);
}

// [spec:pgorm:req:napi.writes/test]
test("an INSERT writes its rows and returns what it wrote", async () => {
  const rows = await pool().query(
    insert(account).columns("id", "name", "mood").values(1, "Alice", new Value("calm", mood)).values(2, "O'Brien", Value.null(mood))
      .returning([col("id"), col("mood")]),
  );
  assert.deepStrictEqual(rows, [{ id: 1n, mood: "calm" }, { id: 2n, mood: null }]);
  assert.deepStrictEqual(await names(), [[1n, "Alice"], [2n, "O'Brien"]]);
});

// [spec:pgorm:req:napi.writes/test]
test("an upsert updates from EXCLUDED, and RETURNING's old version tells inserted rows from updated ones", async () => {
  await pool().execute(insert(account).columns("id", "name").values(1, "Alice"));
  const upsert = insert(account).columns("id", "name").values(1, "Alicia").values(2, "Bob")
    .onConflict(Conflict.on("id").update("name"))
    .returning([col("id"), ReturningRow.old.col("name").as("was"), ReturningRow.old.col("id").isNull().as("inserted")]);
  assert.deepStrictEqual(await pool().query(upsert), [
    { id: 1n, was: "Alice", inserted: false },
    { id: 2n, was: null, inserted: true },
  ]);
  assert.deepStrictEqual(await names(), [[1n, "Alicia"], [2n, "Bob"]]);
});

// [spec:pgorm:req:napi.writes/test]
test("conflict arbiters: any conflict, a named constraint, and a partial index with its predicate", async () => {
  await pool().execute(insert(account).columns("id", "name", "email").values(1, "Alice", "a@x").values(2, "Bob", "b@x"));
  assert.equal(await pool().execute(insert(account).columns("id", "name").values(1, "again").onConflict(Conflict.doNothing())), 0);
  const byConstraint = insert(account).columns("id", "name", "email").values(3, "Carol", "a@x")
    .onConflict(
      Conflict.onConstraint("account_email_key").set("visits", col("visits", { table: "account" }).add(10))
        .where(col("name", { table: "account" }).ne("nobody")),
    );
  assert.equal(await pool().execute(byConstraint), 1);
  assert.equal((await pool().one(select(col("visits")).from(account).where(col("id").eq(1)))).visits, 10);
  const partial = insert(account).columns("id", "name").values(4, "ALICE")
    .onConflict(Conflict.on(call("lower", col("name"))).where(col("active").eq(true)).doNothing());
  assert.equal(await pool().execute(partial), 0);
  await assert.rejects(
    pool().execute(insert(account).columns("id", "name").values(5, "BOB").onConflict(Conflict.on(call("lower", col("name"))).doNothing())),
    (error: unknown) => error instanceof DatabaseError && error.sqlstate === "42P10",
  );
});

// [spec:pgorm:req:napi.writes/test]
test("INSERT takes its rows from a query, or writes one row of defaults, OVERRIDING an identity when told", async () => {
  await pool().execute("INSERT INTO staged VALUES (7, 'Gil'), (8, 'Hal')");
  assert.equal(await pool().execute(insert(account).columns("id", "name").select(select(col("id"), col("name")).from(staged))), 2);
  assert.deepStrictEqual(await names(), [[7n, "Gil"], [8n, "Hal"]]);
  const ticket = new Table("ticket");
  assert.deepStrictEqual(await pool().query(insert(ticket).defaultValues().returning()), [{ id: 1n, note: "none" }]);
  await assert.rejects(
    pool().execute(insert(ticket).columns("id", "note").values(50, "x")),
    (error: unknown) => error instanceof DatabaseError && error.sqlstate === "428C9",
  );
  assert.deepStrictEqual(
    await pool().query(insert(ticket).columns("id", "note").values(50, "x").overriding("systemValue").returning([col("id")])),
    [{ id: 50n }],
  );
});

// [spec:pgorm:req:napi.writes/test]
test("UPDATE returns each row's old and new versions, renamed when the statement asks", async () => {
  await pool().execute(insert(account).columns("id", "name").values(1, "Alice").values(2, "Bob"));
  const renamed = update(account).set("name", col("name").concat("!")).where(col("id").eq(2))
    .returning([ReturningRow.old.col("name")], { oldAs: "before" });
  await assert.rejects(pool().query(renamed), (error: unknown) => error instanceof DatabaseError && error.sqlstate === "42P01");
  const versions = update(account).set("name", col("name").concat("!")).where(col("id").eq(2))
    .returning([ReturningRow.old.col("name").as("was"), ReturningRow.new.col("name").as("now")]);
  assert.deepStrictEqual(await pool().query(versions), [{ was: "Bob", now: "Bob!" }]);
  const both = update(account).set("visits", col("visits").add(1)).where(col("id").eq(1))
    .returning([col("visits", { table: "o" }).as("before"), col("visits", { table: "n" }).as("after")], { oldAs: "o", newAs: "n" });
  assert.deepStrictEqual(await pool().query(both), [{ before: 0, after: 1 }]);
});

// [spec:pgorm:req:napi.writes/test]
test("UPDATE reads FROM another item, and DELETE reads USING one and returns the rows it removed", async () => {
  await pool().execute(insert(account).columns("id", "name").values(1, "Alice").values(2, "Bob").values(3, "Carol"));
  await pool().execute("INSERT INTO staged VALUES (1, 'Alicia'), (3, 'Caroline')");
  const s = new Table("staged", { alias: "s" });
  const renamed = update(account).set("name", s.col("name")).from(s).where(s.col("id").eq(col("id", { table: "account", schema: "app" })));
  assert.equal(await pool().execute(renamed), 2);
  assert.deepStrictEqual(await names(), [[1n, "Alicia"], [2n, "Bob"], [3n, "Caroline"]]);
  const removed = await pool().query(
    deleteFrom(account).using(staged).where(staged.col("id").eq(col("id", { table: "account", schema: "app" })))
      .returning([ReturningRow.old.col("name"), ReturningRow.new.col("name").as("after")]),
  );
  assert.deepStrictEqual(removed.map((row) => [row.name, row.after]).sort(), [["Alicia", null], ["Caroline", null]]);
  assert.deepStrictEqual(await names(), [[2n, "Bob"]]);
});

// [spec:pgorm:req:napi.writes/test]
test("a write with a RETURNING list is a common table expression's body, and a write takes a WITH clause", async () => {
  await pool().execute(insert(account).columns("id", "name").values(1, "Alice").values(2, "Bob").values(3, "Carol"));
  const moved = insert(staged).columns("id", "name").select(select(col("id"), col("name")).from(new Table("gone")))
    .with(new With("gone", deleteFrom(account).where(col("id").gte(2)).returning([col("id"), col("name")])));
  assert.equal(await pool().execute(moved), 2);
  assert.deepStrictEqual(await names(), [[1n, "Alice"]]);
  const counted = await pool().one(
    select(call("count", col("id")).as("n")).from(new Table("touched"))
      .with(new With("touched", update(staged).set("name", "x").allRows().returning([col("id")]))),
  );
  assert.equal(counted.n, 2n);
});

// [spec:pgorm:req:napi.writes/test]
test("a write that cannot run is refused before anything is sent", async () => {
  await pool().execute(insert(account).columns("id", "name").values(1, "Alice"));
  const unguarded = update(account).set("name", "nobody");
  assert.throws(() => unguarded.inspect(), refused(/UPDATE needs where\(\.\.\), or allRows\(\)/));
  await assert.rejects(pool().execute(unguarded), refused(/allRows/));
  await assert.rejects(pool().execute(deleteFrom(account)), refused(/DELETE needs where/));
  await assert.rejects(pool().execute(update(account).allRows()), refused(/at least one set/));
  await assert.rejects(pool().execute(insert(account)), refused(/INSERT needs rows/));
  assert.deepStrictEqual(await names(), [[1n, "Alice"]]);
  assert.equal(await pool().execute(update(account).set("visits", 3).allRows()), 1);
  assert.equal(await pool().execute(deleteFrom(account).allRows()), 1);
});

// [spec:pgorm:req:napi.writes/test]
test("shapes an INSERT, an UPDATE or a conflict cannot take are refused as they are built", () => {
  assert.throws(() => insert(account).values(1), refused(/follows columns/));
  assert.throws(() => insert(account).columns("a", "a"), refused(/"a" is named twice/));
  assert.throws(() => (insert(account).columns as () => unknown).call(insert(account)), refused(/defaultValues/));
  assert.throws(() => insert(account).columns("a").values(1, 2), refused(/mismatch: 1 != 2/));
  assert.throws(() => insert(account).columns("a").values(1).columns("b"), refused(/once, before any row/));
  assert.throws(() => insert(account).columns("a", "b").select(select(col("x"))), refused(/mismatch: 2 != 1/));
  assert.throws(() => insert(account).columns("a").values(1).defaultValues(), refused(/mixes with neither/));
  assert.throws(() => insert(account).defaultValues().columns("a"), refused(/once, before any row/));
  assert.throws(() => insert(account).overriding("always" as never), refused(/overriding/));
  assert.throws(() => update(account).set("a", 1).set("a", 2), refused(/"a" is assigned twice/));
  assert.throws(() => (Conflict.on as () => unknown)(), refused(/at least one column/));
  assert.throws(() => Conflict.on(1 as never), refused(/column name or an index expression/));
  assert.throws(() => (Conflict.on("id").update as () => unknown).call(Conflict.on("id")), refused(/at least one column/));
  assert.throws(() => Conflict.onConstraint("account_pkey").where(col("x").eq(1)), refused(/ON CONSTRAINT takes no predicate/));
  assert.throws(
    () => insert(account).columns("id").values(1).onConflict(Conflict.on("id") as never),
    refused(/needs its action/),
  );
  assert.throws(() => insert(new Table("t")).returning([1 as never]), refused(/RETURNING item/));
  assert.throws(() => insert(col("t") as never), refused(/target is a Table/));
});

// [spec:pgorm:req:napi.writes/test]
test("a write in a transaction rolled back leaves nothing", async () => {
  await assert.rejects(
    pool().transaction(async (transaction) => {
      await transaction.execute(insert(account).columns("id", "name").values(9, "Ghost"));
      assert.equal((await transaction.query(select().from(account))).length, 1);
      throw new Error("roll back");
    }),
    /roll back/,
  );
  assert.deepStrictEqual(await names(), []);
});
