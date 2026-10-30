// Schemas built from JavaScript against a live server, in either runtime:
// created, holding a table qualified by them, renamed, dropped, and what the
// server refuses along the way.

import assert from "node:assert/strict";
import { after, before, test } from "node:test";

import {
  ColumnDef,
  ConstructionError,
  createSchema,
  createTable,
  DatabaseError,
  dropSchema,
  Pool,
  renameSchema,
  Table,
} from "../lib/index.js";
import { scratchDatabase } from "./support.ts";

let database: { dsn: string; drop(): Promise<void> } | undefined;
let shared: Pool | undefined;

function pool(): Pool {
  if (!shared) throw new Error("the scratch database was not created");
  return shared;
}

before(async () => {
  database = await scratchDatabase("pgorm_napi_schema_schemas");
  shared = new Pool(database.dsn, { maxSize: 2 });
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

function rejectedWith(sqlstate: string): (error: unknown) => true {
  return (error: unknown) => {
    assert.ok(error instanceof DatabaseError, `expected a DatabaseError, got ${error}`);
    assert.equal(error.sqlstate, sqlstate, error.message);
    return true;
  };
}

/** Whether a schema of exactly this name exists. */
async function exists(name: string): Promise<boolean> {
  return (await pool().optional("SELECT 1 FROM pg_namespace WHERE nspname = $1", [name])) !== null;
}

// [spec:pgorm:req:bindings.schema-schemas/test]
test("each schema statement names its schemas as quoted identifiers and binds nothing", () => {
  assert.deepStrictEqual(createSchema('bill "x"', { ifNotExists: true }).inspect(), {
    sql: 'CREATE SCHEMA IF NOT EXISTS "bill ""x"""',
    values: [],
  });
  assert.equal(dropSchema(["a", "B"], { ifExists: true, behavior: "cascade" }).inspect().sql, 'DROP SCHEMA IF EXISTS "a", "B" CASCADE');
  assert.equal(dropSchema("a").inspect().sql, 'DROP SCHEMA "a"');
  assert.equal(renameSchema("a", "b").inspect().sql, 'ALTER SCHEMA "a" RENAME TO "b"');

  assert.throws(() => dropSchema([] as never), refused(/at least one schema/));
  assert.throws(() => createSchema(new Table("t") as never), refused(/identifier is a string/));
  assert.throws(() => createSchema(""), refused(/1–63 UTF-8 bytes/));
  assert.throws(() => renameSchema("a", "x".repeat(64)), refused(/1–63 UTF-8 bytes/));
  assert.throws(() => dropSchema(["a", 3] as never), refused(/identifier is a string/));
  assert.throws(() => createSchema("a", { ifExists: true } as never), TypeError);
  assert.throws(() => dropSchema("a", { behavior: "sideways" } as never));
});

// [spec:pgorm:req:bindings.schema-schemas/test]
test("a schema holds a table qualified by it, carries it through a rename, and is dropped only by a cascade", async () => {
  await pool().execute(createSchema('billing "雪"'));
  assert.equal(await exists('billing "雪"'), true);
  const invoice = new Table("invoice", { schema: 'billing "雪"' });
  await pool().execute(createTable(invoice).column(new ColumnDef("id", "integer")));
  await pool().execute('INSERT INTO "billing ""雪"""."invoice" VALUES (1)');

  await assert.rejects(pool().execute(dropSchema('billing "雪"')), rejectedWith("2BP01"));
  await assert.rejects(pool().execute(dropSchema('billing "雪"', { behavior: "restrict" })), rejectedWith("2BP01"));

  await pool().execute(renameSchema('billing "雪"', "ledger"));
  assert.equal(await exists('billing "雪"'), false);
  const row = await pool().optional("SELECT id FROM ledger.invoice");
  assert.equal(row?.id, 1);

  await pool().execute(dropSchema("ledger", { behavior: "cascade" }));
  assert.equal(await exists("ledger"), false);
});

// [spec:pgorm:req:bindings.schema-schemas/test]
test("the server refuses a schema that exists, a pg_ name and a missing schema", async () => {
  await pool().execute(createSchema("archive"));
  await assert.rejects(pool().execute(createSchema("archive")), rejectedWith("42P06"));
  await pool().execute(createSchema("archive", { ifNotExists: true }));
  await assert.rejects(pool().execute(createSchema("pg_archive")), rejectedWith("42939"));
  await assert.rejects(pool().execute(renameSchema("missing", "other")), rejectedWith("3F000"));
  await assert.rejects(pool().execute(dropSchema("missing")), rejectedWith("3F000"));
  await pool().execute(dropSchema(["missing", "archive"], { ifExists: true }));
  assert.equal(await exists("archive"), false);
});
