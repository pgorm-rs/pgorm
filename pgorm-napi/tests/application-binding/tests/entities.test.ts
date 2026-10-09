// The application's registered entities, through its own native module,
// against a live server in either runtime: reads through `Select<E>`, writes
// through `ActiveModelTrait` with the application's hooks, the terminals
// that return a row's two versions, and registered graphs and their cursors.
// [spec:pgorm:req:napi.application/test]

import assert from "node:assert/strict";
import { after, before, beforeEach, test } from "node:test";

import {
  Conflict,
  ConstructionError,
  DecodeError,
  entities,
  entity,
  graph,
  graphs,
  LifecycleError,
  Pool,
  TypeName,
} from "../../../lib/index.js";
import { scratchDatabase } from "../../support.ts";

let database: { dsn: string; drop(): Promise<void> } | undefined;
let shared: Pool | undefined;

function pool(): Pool {
  if (!shared) throw new Error("the scratch database was not created");
  return shared;
}

const Account = entity("app.Account");
const Note = entity("app.Note");
const Membership = entity("app.Membership");

before(async () => {
  database = await scratchDatabase("pgorm_napi_entities");
  shared = new Pool(database.dsn, { maxSize: 4 });
  for (
    const sql of [
      "CREATE SCHEMA napi_entities",
      "CREATE TYPE napi_entities.mood AS ENUM ('calm', 'busy')",
      `CREATE TABLE napi_entities.accounts (
         id int4 PRIMARY KEY, "display name" text NOT NULL, note text, version int4 NOT NULL,
         mood napi_entities.mood NOT NULL
       )`,
      "CREATE TABLE napi_entities.notes (id int4 PRIMARY KEY, account_id int4 NOT NULL, body text NOT NULL)",
      `CREATE TABLE napi_entities.memberships (
         account_id int4, team text, role text NOT NULL, PRIMARY KEY (account_id, team)
       )`,
    ]
  ) {
    await pool().execute(sql);
  }
});

beforeEach(async () => {
  await pool().execute("TRUNCATE napi_entities.accounts, napi_entities.notes, napi_entities.memberships");
});

after(async () => {
  await shared?.close();
  await database?.drop();
});

async function seed(): Promise<void> {
  await pool().execute(
    `INSERT INTO napi_entities.accounts VALUES (1, 'Ann', NULL, 1, 'calm'), (2, 'Bob', 'b', 1, 'busy'),
       (3, 'Cy', NULL, 1, 'calm')`,
  );
  await pool().execute("INSERT INTO napi_entities.notes VALUES (10, 1, 'a1'), (11, 1, 'a2'), (12, 2, 'b1')");
}

function refused(pattern: RegExp): (error: unknown) => true {
  return (error: unknown) => {
    assert.ok(error instanceof ConstructionError, `expected a ConstructionError, got ${error}`);
    assert.match(error.message, pattern);
    return true;
  };
}

// [spec:pgorm:req:napi.entities/test]
test("the module names its registrations and describes each, and refuses a name it does not register", () => {
  assert.deepStrictEqual(entities(), ["app.Account", "app.Membership", "app.Note"]);
  assert.deepStrictEqual(graphs(), ["app.AccountNotes", "app.AccountOnly", "app.MembershipOnly", "app.MixedNotes", "app.RequiredNotes"]);
  assert.throws(() => entity("app.Missing"), refused(/no entity is registered as "app.Missing"/));
  assert.throws(() => graph("app.Missing"), refused(/no graph is registered as "app.Missing"/));
  assert.deepStrictEqual(Account.columns, ["id", "display name", "note", "version", "mood"]);
  const described = Account.describe();
  assert.equal(described.name, "app.Account");
  assert.equal(described.schema, "napi_entities");
  assert.equal(described.table, "accounts");
  assert.deepStrictEqual(described.primaryKey, ["id"]);
  assert.deepStrictEqual(
    described.columns.map((column) => [column.name, column.kind, column.nullable]),
    [["id", "i32", false], ["display name", "text", false], ["note", "text", true], ["version", "i32", false], [
      "mood",
      'enum "napi_entities"."mood"',
      false,
    ]],
  );
  assert.equal(described.relations[0].type, "hasMany");
  assert.match(described.rust.entity, /account::Entity$/);
  assert.deepStrictEqual(Membership.describe().primaryKey, ["account_id", "team"]);
  assert.deepStrictEqual(graph("app.MixedNotes").describe().sources.map((source: { slot: string }) => source.slot), [
    "root",
    "Req",
    "Opt",
  ]);
});

// [spec:pgorm:req:napi.entity-reads/test]
test("a query reads records through Select's terminals, keyed by SQL column", async () => {
  await seed();
  const busy = await Account.find().where(Account.col("mood").eq("busy")).all(pool());
  assert.deepStrictEqual(busy, [{ id: 2, "display name": "Bob", note: "b", version: 1, mood: "busy" }]);
  assert.ok(Object.isFrozen(busy[0]));
  const ordered = await Account.find().orderBy(Account.col("id").desc()).limit(2).all(pool());
  assert.deepStrictEqual(ordered.map((record) => record.id), [3, 2]);
  assert.equal((await Account.find().where(Account.col("id").gte(2)).orderBy(Account.col("id").asc()).one(pool())).id, 2);
  assert.equal((await Account.find().orderBy(Account.col("id").asc()).offset(1).oneOpt(pool()))?.id, 2);
  assert.equal(await Account.find().where(Account.col("id").gt(9)).oneOpt(pool()), null);
  await assert.rejects(Account.find().where(Account.col("id").gt(9)).one(pool()), DecodeError);
  assert.match(Account.find().inspect("one").sql, /LIMIT \$1$/);
  assert.match(Account.col("mood").eq("busy").inspect().sql, /= CAST\(\$1::text AS napi_entities\.mood\)/);
  assert.throws(() => Account.col("mood").eq(null as never), refused(/compared with null is never true/));
  assert.throws(() => Account.col("id").eq("one"), ConstructionError);
  assert.throws(() => Account.col("nickname"), refused(/no column "nickname"/));
  const counted = await pool().one("SELECT count(*) AS n FROM napi_entities.accounts WHERE id = $1", [2]);
  assert.equal(counted.n, 1n);
});

// [spec:pgorm:req:napi.entity-writes/test]
test("an ActiveModel holds each column's state, and its writes run the application's hooks", async () => {
  const fresh = Account.active();
  assert.deepStrictEqual(fresh.get("version"), { state: "set", value: 1 });
  assert.deepStrictEqual(fresh.get("mood"), { state: "set", value: "calm" });
  assert.deepStrictEqual(fresh.get("note"), { state: "notSet" });
  const ann = fresh.set("id", 1).set("display name", "Ann").set("note", "x").notSet("note");
  assert.deepStrictEqual(ann.get("note"), { state: "notSet" });
  assert.deepStrictEqual(fresh.get("display name"), { state: "notSet" });
  const inserted = await ann.insert(pool());
  assert.deepStrictEqual(inserted, { id: 1, "display name": "Ann|before", note: null, version: 1, mood: "calm" });
  const active = Account.intoActive(inserted);
  assert.deepStrictEqual(active.get("display name"), { state: "unchanged", value: "Ann|before" });
  const updated = await active.set("note", "seen").update(pool());
  assert.equal(updated.version, 2);
  assert.equal(updated.note, "seen");
  assert.deepStrictEqual(active.reset("note").get("note"), { state: "set", value: null });
  assert.deepStrictEqual(active.set("note", "y").get("note"), { state: "set", value: "y" });
  await assert.rejects(
    Account.active().set("id", 2).set("display name", "reject_before").insert(pool()),
    refused(/before_save refused/),
  );
  await assert.rejects(
    Account.active().set("id", 3).set("display name", "reject_after").insert(pool()),
    refused(/after_save refused/),
  );
  assert.deepStrictEqual((await Account.find().orderBy(Account.col("id").asc()).all(pool())).map((record) => record.id), [1, 3]);
  const doomed = await Account.active().set("id", 99).set("display name", "Zed").insert(pool());
  await assert.rejects(Account.intoActive(doomed).delete(pool()), refused(/before_delete refused/));
  assert.equal(await Account.intoActive(inserted).delete(pool()), 1);
  assert.throws(() => Account.active().set("id", "x"), ConstructionError);
  assert.throws(() => Account.active().set("mood", "wild"), ConstructionError);
});

// [spec:pgorm:req:napi.entity-writes/test]
test("a record's model converts and copies as Rust's does, and belongs to its entity", async () => {
  await seed();
  const bob = await Account.find().where(Account.col("id").eq(2)).one(pool());
  const renamed = Account.withValue(bob, "display name", "Robert");
  assert.equal(renamed["display name"], "Robert");
  assert.equal(bob["display name"], "Bob");
  const mood = Account.tagged(bob, "mood");
  assert.equal(mood.kind, "enum");
  assert.ok(mood.typeName instanceof TypeName);
  assert.equal(String(mood.typeName), '"napi_entities"."mood"');
  assert.deepStrictEqual(Account.intoActive(renamed).get("display name"), { state: "unchanged", value: "Robert" });
  assert.throws(() => Account.intoActive({ ...bob }), TypeError);
  const note = await Note.find().oneOpt(pool());
  assert.ok(note);
  assert.throws(() => Account.intoActive(note), refused(/app.Note's, not app.Account's/));
  assert.throws(() => Account.update(Note.active()), refused(/app.Note's, not app.Account's/));
});

// [spec:pgorm:req:napi.entity-versions/test]
test("the version terminals return each written row before and after, no hook running", async () => {
  await seed();
  const ann = await Account.find().where(Account.col("id").eq(1)).one(pool());
  const change = await Account.update(Account.intoActive(ann).set("note", "changed")).returningChange(pool());
  assert.equal(change.old.note, null);
  assert.equal(change.new.note, "changed");
  assert.equal(change.new.version, 1);
  const changes = await Account.updateMany().set("note", "bulk").set("version", Account.col("version").add(10))
    .where(Account.col("mood").eq("calm")).returningChanges(pool());
  changes.sort((left, right) => Number(left.old.id) - Number(right.old.id));
  assert.deepStrictEqual(changes.map(({ old, new: now }) => [old.id, old.note, now.note, now.version]), [
    [1, "changed", "bulk", 11],
    [3, null, "bulk", 11],
  ]);
  await assert.rejects(Account.updateMany().set("note", "x").returningChanges(pool()), refused(/where\(\.\.\) or allRows\(\)/));
  await assert.rejects(Account.updateMany().allRows().returningChanges(pool()), ConstructionError);
  assert.equal((await Account.updateMany().set("note", "all").allRows().returningChanges(pool())).length, 3);
  const renamed = Conflict.on("id").update("display name");
  const updated = await Account.insert(Account.active().set("id", 1).set("display name", "Anne")).onConflict(renamed)
    .returningUpsert(pool());
  assert.ok(updated?.kind === "updated");
  assert.equal(updated.old["display name"], "Ann");
  assert.equal(updated.new["display name"], "Anne");
  const inserted = await Account.insert(Account.active().set("id", 7).set("display name", "Gil")).onConflict(renamed)
    .returningUpsert(pool());
  assert.ok(inserted?.kind === "inserted");
  assert.equal(inserted.new["display name"], "Gil");
  assert.equal(
    await Account.insert(Account.active().set("id", 7).set("display name", "Hal")).onConflict(Conflict.doNothing())
      .returningUpsert(pool()),
    null,
  );
  const batch = await Account.insertMany([
    Account.active().set("id", 2).set("display name", "Bobby"),
    Account.active().set("id", 8).set("display name", "Ivy"),
  ]).onConflict(renamed).returningUpserts(pool());
  assert.deepStrictEqual(batch.map((row) => [row.kind, row.new["display name"]]), [["updated", "Bobby"], ["inserted", "Ivy"]]);
  assert.deepStrictEqual(await Account.insertMany([]).returningUpserts(pool()), []);
});

// [spec:pgorm:req:napi.entity-graphs/test]
test("a registered graph decodes its root and slots, an absent optional slot null", async () => {
  await seed();
  type Pair = [Record<string, unknown>, Record<string, unknown> | null];
  const optional = await graph("app.AccountNotes").find({ aliases: ["n"] }).orderBy(Account.col("id").asc()).all(pool()) as Pair[];
  assert.deepStrictEqual(optional.map(([account, note]) => [account.id, note?.body ?? null]), [
    [1, "a1"],
    [1, "a2"],
    [2, "b1"],
    [3, null],
  ]);
  const required = await graph("app.RequiredNotes").find().all(pool());
  assert.equal(required.length, 3);
  const roots = await graph("app.AccountOnly").find().all(pool());
  assert.equal(roots.length, 3);
  assert.ok(!Array.isArray(roots[0]));
  const mixed = graph("app.MixedNotes").find();
  assert.deepStrictEqual(mixed.aliases, ["g1", "g2"]);
  const query = graph("app.AccountNotes").find({ aliases: ["n"] });
  const filtered = await query.where(query.col(1, "body").eq("b1")).oneOpt(pool()) as Pair | null;
  assert.equal(filtered?.[0].id, 2);
  assert.equal(await query.where(query.col(0, "id").eq(99)).oneOpt(pool()), null);
  assert.throws(() => query.col(2, "id"), refused(/no source 2/));
  assert.throws(() => query.col(1, "nope"), refused(/no column "nope"/));
  assert.throws(() => graph("app.AccountNotes").find({ aliases: ["a", "b"] }), refused(/joins 1 slots, and 2 aliases/));
  assert.throws(() => graph("app.AccountNotes").find({ aliases: ["accounts"] }), refused(/names a source twice/));
});

// [spec:pgorm:req:napi.entity-graphs/test]
test("a registered graph's cursor pages by keyset, resuming inside a root's slot rows", async () => {
  await seed();
  const cursor = graph("app.AccountNotes").find({ aliases: ["n"] }).cursor("id");
  const first = await cursor.first(1).all(pool()) as [Record<string, unknown>, Record<string, unknown> | null][];
  assert.deepStrictEqual(first.map(([account, note]) => [account.id, note?.id]), [[1, 10]]);
  const resumed = await cursor.afterWith(1, 10).first(5).all(pool()) as [Record<string, unknown>, unknown][];
  assert.deepStrictEqual(resumed.map(([account]) => account.id), [1, 2, 3]);
  const skipped = await cursor.after(1).first(5).all(pool()) as [Record<string, unknown>, unknown][];
  assert.deepStrictEqual(skipped.map(([account]) => account.id), [2, 3]);
  const last = await cursor.last(2).all(pool()) as [Record<string, unknown>, unknown][];
  assert.deepStrictEqual(last.map(([account]) => account.id), [2, 3]);
  const descending = await cursor.desc().first(1).all(pool()) as [Record<string, unknown>, unknown][];
  assert.deepStrictEqual(descending.map(([account]) => account.id), [3]);
  assert.throws(() => cursor.afterWith(1), refused(/takes 2 values, and 1 were given/));
  assert.throws(() => cursor.after("x"), ConstructionError);
});

// [spec:pgorm:req:napi.entities/test]
test("an entity's operations run on a connection or a transaction, with a statement's refusals and aborts", async () => {
  await seed();
  await assert.rejects(
    pool().transaction(async (transaction) => {
      await Account.active().set("id", 50).set("display name", "Tx").insert(transaction);
      assert.equal((await Account.find().where(Account.col("id").eq(50)).all(transaction)).length, 1);
      throw new Error("roll back");
    }),
    /roll back/,
  );
  assert.equal(await Account.find().where(Account.col("id").eq(50)).oneOpt(pool()), null);
  await pool().connection(async (connection) => {
    const slow = pool().query("SELECT 1");
    const reads = [Account.find().all(connection), Account.find().all(connection)];
    const outcomes = await Promise.allSettled(reads);
    assert.ok(outcomes.some((outcome) => outcome.status === "rejected" && outcome.reason instanceof LifecycleError));
    await slow;
  });
  const signal = AbortSignal.abort(new Error("stop"));
  await assert.rejects(Account.find().all(pool(), { signal }), /stop/);
  await assert.rejects(Account.find().all({} as never), TypeError);
});
