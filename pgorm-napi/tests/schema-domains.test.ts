// Domains built from JavaScript against a live server, in either runtime:
// created over a base type with a default, a NOT NULL and checks, a table's
// column typed with one, each ALTER DOMAIN action, a NOT VALID check then
// validated, dropped, and what the server refuses along the way.

import assert from "node:assert/strict";
import { after, before, test } from "node:test";

import {
  alterDomain,
  ColumnDef,
  ConstructionError,
  createDomain,
  createSchema,
  createTable,
  DatabaseError,
  DataType,
  domainValue,
  dropDomain,
  Pool,
  Table,
  TypeName,
} from "../lib/index.js";
import { scratchDatabase } from "./support.ts";

let database: { dsn: string; drop(): Promise<void> } | undefined;
let shared: Pool | undefined;

function pool(): Pool {
  if (!shared) throw new Error("the scratch database was not created");
  return shared;
}

before(async () => {
  database = await scratchDatabase("pgorm_napi_schema_domains");
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

/** The domain's constraints, by name, as the catalog holds them. */
async function constraints(domain: string): Promise<string[]> {
  const rows = await pool().query(
    "SELECT conname FROM pg_constraint WHERE contypid = $1::text::regtype ORDER BY conname",
    [domain],
  );
  return rows.map((row) => String(row.conname));
}

// [spec:pgorm:req:napi.schema-domains/test]
test("each domain statement names its domain as a type, renders its clauses in order and binds nothing", () => {
  const quantity = new TypeName("quantity", { schema: "billing" });
  assert.deepStrictEqual(
    createDomain(quantity, "integer")
      .check(domainValue().gt(0), { name: 'pos "x"' })
      .notNull()
      .null()
      .default(2)
      .default(1)
      .collate("C")
      .inspect(),
    {
      sql: 'CREATE DOMAIN "billing"."quantity" AS integer COLLATE "C" DEFAULT 1 NULL CONSTRAINT "pos ""x""" CHECK (VALUE > 0)',
      values: [],
    },
  );
  assert.equal(
    createDomain("tags", new DataType("text").array()).check(domainValue().isNotNull()).inspect().sql,
    'CREATE DOMAIN "tags" AS text[] CHECK (VALUE IS NOT NULL)',
  );
  assert.equal(alterDomain(quantity).dropNotNull().inspect().sql, 'ALTER DOMAIN "billing"."quantity" DROP NOT NULL');
  assert.equal(
    dropDomain([quantity, "tags"], { ifExists: true, behavior: "cascade" }).inspect().sql,
    'DROP DOMAIN IF EXISTS "billing"."quantity", "tags" CASCADE',
  );

  assert.throws(() => dropDomain([] as never), refused(/at least one domain/));
  assert.throws(() => createDomain(new Table("t") as never, "integer"), refused(/string or a TypeName/));
  assert.throws(() => createDomain("d", "nonsense" as never), ConstructionError);
  assert.throws(() => createDomain("", "integer"), refused(/1–63 UTF-8 bytes/));
  assert.throws(() => alterDomain("d").renameTo("x".repeat(64)), refused(/1–63 UTF-8 bytes/));
  assert.throws(() => createDomain("d", "integer").check(1 as never), refused(/expected an expression/));
  assert.throws(
    () => createDomain("d", "integer").check(domainValue().gt(0), { notValid: true } as never),
    TypeError,
  );
  assert.throws(() => alterDomain("d").dropConstraint("c", { behavior: "sideways" } as never), ConstructionError);
});

// [spec:pgorm:req:napi.schema-domains/test]
test("an ALTER DOMAIN with no action has no SQL, and a terminal refuses it", async () => {
  const pending = alterDomain("quantity");
  assert.equal("inspect" in pending, false);
  await assert.rejects(pool().execute(pending as never), refused(/ALTER DOMAIN needs an action/));
});

// [spec:pgorm:req:napi.schema-domains/test]
test("a column typed with a domain takes its default and is held to its checks and NOT NULL", async () => {
  await pool().execute(createSchema("billing"));
  const quantity = new TypeName("quantity", { schema: "billing" });
  await pool().execute(
    createDomain(quantity, "integer")
      .default(1)
      .notNull()
      .check(domainValue().gt(0), { name: "positive" })
      .check(domainValue().lt(1000)),
  );
  assert.deepStrictEqual(await constraints("billing.quantity"), ["positive", "quantity_check", "quantity_not_null"]);
  const line = new Table("line", { schema: "billing" });
  await pool().execute(
    createTable(line)
      .column(new ColumnDef("id", "integer"))
      .column(new ColumnDef("qty", quantity)),
  );
  await pool().execute("INSERT INTO billing.line (id) VALUES (1)");
  await pool().execute("INSERT INTO billing.line (id, qty) VALUES ($1, $2)", [2, 7]);
  const rows = await pool().query("SELECT id, qty FROM billing.line ORDER BY id");
  assert.deepStrictEqual(rows.map((row) => [row.id, row.qty]), [[1, 1], [2, 7]]);

  await assert.rejects(
    pool().execute("INSERT INTO billing.line (id, qty) VALUES ($1, $2)", [3, 0]),
    rejectedWith("23514"),
  );
  await assert.rejects(
    pool().execute("INSERT INTO billing.line (id, qty) VALUES ($1, $2)", [3, 5000]),
    rejectedWith("23514"),
  );
  await assert.rejects(pool().execute("INSERT INTO billing.line (id, qty) VALUES (3, NULL)"), rejectedWith("23502"));

  const small = new TypeName("small_quantity", { schema: "billing" });
  await pool().execute(createDomain(small, quantity).check(domainValue().lt(10)));
  await assert.rejects(pool().execute("SELECT 12::billing.small_quantity"), rejectedWith("23514"));
  await assert.rejects(pool().execute("SELECT 0::billing.small_quantity"), rejectedWith("23514"));
  assert.equal((await pool().one("SELECT 9::billing.small_quantity AS v")).v, 9);
});

// [spec:pgorm:req:napi.schema-domains/test]
test("each ALTER DOMAIN action changes the domain, a NOT VALID check validated once the values pass", async () => {
  await pool().execute(createDomain("score", "integer"));
  await pool().execute(createTable("game").column(new ColumnDef("points", new TypeName("score"))));
  await pool().execute("INSERT INTO game VALUES (-5), (NULL)");

  await pool().execute(alterDomain("score").setDefault(3));
  await pool().execute("INSERT INTO game DEFAULT VALUES");
  assert.equal((await pool().one("SELECT count(*)::int AS n FROM game WHERE points = 3")).n, 1);
  await pool().execute(alterDomain("score").dropDefault());
  assert.equal((await pool().one("SELECT typdefault FROM pg_type WHERE typname = 'score'")).typdefault, null);

  await assert.rejects(pool().execute(alterDomain("score").setNotNull()), rejectedWith("23502"));
  await pool().execute("DELETE FROM game WHERE points IS NULL");
  await pool().execute(alterDomain("score").setNotNull());
  await assert.rejects(pool().execute("INSERT INTO game VALUES (NULL)"), rejectedWith("23502"));
  await pool().execute(alterDomain("score").dropNotNull());
  await pool().execute("INSERT INTO game VALUES (NULL)");

  await assert.rejects(pool().execute(alterDomain("score").addCheck(domainValue().gte(0))), rejectedWith("23514"));
  await pool().execute(alterDomain("score").addCheck(domainValue().gte(0), { name: "non_negative", notValid: true }));
  await assert.rejects(pool().execute("INSERT INTO game VALUES (-1)"), rejectedWith("23514"));
  await assert.rejects(pool().execute(alterDomain("score").validateConstraint("non_negative")), rejectedWith("23514"));
  await pool().execute("DELETE FROM game WHERE points < 0");
  await pool().execute(alterDomain("score").validateConstraint("non_negative"));
  const validated = await pool().one(
    "SELECT convalidated FROM pg_constraint WHERE conname = 'non_negative' AND contypid = 'score'::regtype",
  );
  assert.equal(validated.convalidated, true);

  await pool().execute(alterDomain("score").renameConstraint("non_negative", "at_least_zero"));
  assert.deepStrictEqual(await constraints("score"), ["at_least_zero"]);
  await pool().execute(alterDomain("score").dropConstraint("at_least_zero"));
  await pool().execute(alterDomain("score").dropConstraint("at_least_zero", { ifExists: true, behavior: "restrict" }));
  assert.deepStrictEqual(await constraints("score"), []);

  await pool().execute(alterDomain("score").renameTo("points"));
  const typed = await pool().one(
    "SELECT format_type(atttypid, atttypmod) AS type FROM pg_attribute WHERE attrelid = 'game'::regclass AND attname = 'points'",
  );
  assert.equal(typed.type, "points");
});

// [spec:pgorm:req:napi.schema-domains/test]
test("a drop is refused under RESTRICT while a column is typed with the domain, until it cascades", async () => {
  await pool().execute(createDomain("label", "text").collate("C"));
  await pool().execute(createTable("tagged").column(new ColumnDef("id", "integer")).column(new ColumnDef("tag", new TypeName("label"))));

  await assert.rejects(pool().execute(dropDomain("label")), rejectedWith("2BP01"));
  await assert.rejects(pool().execute(dropDomain("label", { behavior: "restrict" })), rejectedWith("2BP01"));
  await pool().execute(dropDomain("label", { behavior: "cascade" }));
  const columns = await pool().query(
    "SELECT attname FROM pg_attribute WHERE attrelid = 'tagged'::regclass AND attnum > 0 AND NOT attisdropped",
  );
  assert.deepStrictEqual(columns.map((row) => row.attname), ["id"]);
});

// [spec:pgorm:req:napi.schema-domains/test]
test("the server refuses a domain that exists, a missing domain or constraint and a collation its base has none of", async () => {
  await pool().execute(createDomain("amount", "numeric"));
  await assert.rejects(pool().execute(createDomain("amount", "integer")), rejectedWith("42710"));
  await pool().execute(createDomain("other_amount", "integer"));
  await assert.rejects(pool().execute(alterDomain("other_amount").renameTo("amount")), rejectedWith("42710"));
  await pool().execute(alterDomain("amount").addCheck(domainValue().gt(0), { name: "positive" }));
  await assert.rejects(
    pool().execute(alterDomain("amount").addCheck(domainValue().lt(10), { name: "positive" })),
    rejectedWith("42710"),
  );

  await assert.rejects(pool().execute(alterDomain("missing").dropDefault()), rejectedWith("42704"));
  await assert.rejects(pool().execute(dropDomain("missing")), rejectedWith("42704"));
  await assert.rejects(pool().execute(alterDomain("amount").validateConstraint("missing")), rejectedWith("42704"));
  await assert.rejects(pool().execute(alterDomain("amount").dropConstraint("missing")), rejectedWith("42704"));
  await pool().execute(dropDomain(["missing", "amount", "other_amount"], { ifExists: true }));
  assert.equal(await pool().optional("SELECT 1 FROM pg_type WHERE typname = 'amount'"), null);

  await assert.rejects(pool().execute(createDomain("narrow", "integer").collate("C")), rejectedWith("42804"));
});
