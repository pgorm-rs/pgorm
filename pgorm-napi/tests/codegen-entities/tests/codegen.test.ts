// The generated module of the application's registrations, against a live
// server in either runtime: its exports are the registrations they name, and
// their records, graph rows and source rows read as its declarations type
// them.
// [spec:pgorm:req:napi.codegen/test]

import assert from "node:assert/strict";
import { after, before, test } from "node:test";

import { Decimal, pipeline as pl, Pool, Uuid } from "../lib/index.js";
import { Account, AccountNotes, AccountOnly, AccountWithNote, MixedNotes, Note, Sample } from "../lib/app.js";
import { typed } from "./consumer.ts";
import { scratchDatabase } from "./support.ts";

let database: { dsn: string; drop(): Promise<void> } | undefined;
let shared: Pool | undefined;

function pool(): Pool {
  if (!shared) throw new Error("the scratch database was not created");
  return shared;
}

before(async () => {
  database = await scratchDatabase("pgorm_napi_codegen");
  shared = new Pool(database.dsn, { maxSize: 2 });
  for (
    const sql of [
      "CREATE SCHEMA napi_entities",
      "CREATE TYPE napi_entities.mood AS ENUM ('calm', 'busy')",
      `CREATE TABLE napi_entities.accounts (
         id int4 PRIMARY KEY, "display name" text NOT NULL, note text, version int4 NOT NULL,
         mood napi_entities.mood NOT NULL
       )`,
      "CREATE TABLE napi_entities.notes (id int4 PRIMARY KEY, account_id int4 NOT NULL, body text NOT NULL)",
      `CREATE TABLE napi_entities.samples (
         id int8 PRIMARY KEY, small int2 NOT NULL, ratio float8 NOT NULL, flag bool NOT NULL,
         price numeric NOT NULL, token uuid NOT NULL, doc jsonb NOT NULL, day date NOT NULL,
         at timestamptz NOT NULL, raw bytea NOT NULL, tags text[] NOT NULL, note text
       )`,
      "INSERT INTO napi_entities.accounts VALUES (1, 'Ann', NULL, 1, 'calm'), (2, 'Bob', 'b', 1, 'busy')",
      "INSERT INTO napi_entities.notes VALUES (10, 1, 'a1'), (11, 1, 'a2')",
    ]
  ) {
    await pool().execute(sql);
  }
});

after(async () => {
  await shared?.close();
  await database?.drop();
});

// [spec:pgorm:req:napi.codegen/test]
test("the generated exports are the registrations they name", () => {
  assert.equal(Account.name, "app.Account");
  assert.equal(Note.name, "app.Note");
  assert.equal(AccountNotes.name, "app.AccountNotes");
  assert.equal(AccountWithNote.name, "app.AccountWithNote");
  assert.equal(typeof typed, "function");
});

// [spec:pgorm:req:napi.codegen-types/test]
test("records, graph rows and source rows read as the declarations type them", async () => {
  const busy = await Account.find().where(Account.col("mood").eq("busy")).one(pool());
  assert.equal(busy["display name"], "Bob");
  const sample = await Sample.active()
    .set("id", 1n).set("small", 2).set("ratio", 0.5).set("flag", true).set("price", new Decimal("19.90"))
    .set("token", new Uuid("0190f2b5-7f2c-7f3a-8f00-000000000001")).set("doc", { a: [1, 2] })
    .set("day", Temporal.PlainDate.from("2026-10-09")).set("at", Temporal.Instant.from("2026-10-09T12:00:00Z"))
    .set("raw", new Uint8Array([1, 2])).set("tags", ["x", "y"]).set("note", null)
    .insert(pool());
  assert.equal(sample.id, 1n);
  assert.ok(sample.price instanceof Decimal);
  assert.equal(String(sample.price), "19.90");
  assert.ok(sample.day instanceof Temporal.PlainDate);
  assert.ok(sample.at instanceof Temporal.Instant);
  assert.deepStrictEqual(sample.tags, ["x", "y"]);
  const rows = await AccountNotes.find({ aliases: ["n"] }).orderBy(Account.col("id").asc()).all(pool());
  assert.deepStrictEqual(rows.map(([account, note]) => [account.id, note?.body ?? null]), [[1, "a1"], [1, "a2"], [2, null]]);
  assert.equal((await AccountOnly.find().all(pool())).length, 2);
  assert.equal((await MixedNotes.find().all(pool())).length, 4);
  const selected = await pl.from(Account)
    .join(pl.source(Note).named("n"), pl.col("accounts", "id").eq(pl.col("n", "account_id")), { kind: "left" })
    .sort(pl.col("accounts", "id"), pl.col("n", "id"))
    .selectSources(AccountWithNote, { qualifiers: ["accounts", "n"] })
    .all(pool());
  assert.deepStrictEqual(selected.map(([account, note]) => [account?.id, note?.id ?? null]), [[1, 10], [1, 11], [2, null]]);
});
