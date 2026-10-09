// A bound query through pgorm's pool settles a promise, in either runtime.
//
// The suite uses the public API through its declarations, which `deno test`
// and `deno check` type-check before anything runs.
// [spec:pgorm:req:napi.typing/test]

import assert from "node:assert/strict";
import { after, test } from "node:test";

import {
  ConnectionError,
  ConstructionError,
  DatabaseError,
  DecodeError,
  PgormError,
  Pool,
  version,
} from "../lib/index.js";
import { dsn } from "./support.ts";

const pool = new Pool(dsn(), { maxSize: 16 });

after(async () => {
  await pool.close();
});

async function scalar(sql: string, params: readonly (number | bigint | string)[] = []): Promise<unknown> {
  return (await pool.one(sql, params)).n;
}

// [spec:pgorm:req:napi.loading+1/test]
test("the addon loads and reports the pgorm release it was built from", () => {
  assert.match(version, /^\d+\.\d+\.\d+/);
});

// [spec:pgorm:req:napi.promises/test]
test("a bound query resolves with its result", async () => {
  assert.equal(await scalar("SELECT $1::int + 1 AS n", [41]), 42);
});

// [spec:pgorm:req:napi.promises/test]
test("a hundred queries in flight at once each resolve with their own result", async () => {
  const values = Array.from({ length: 100 }, (_, index) => index * 1000 - 50_000);
  const pending = values.map((value) => scalar("SELECT $1::int + 1 AS n", [value]));
  assert.deepEqual(await Promise.all(pending), values.map((value) => value + 1));
});

// [spec:pgorm:req:napi.promises/test]
test("queries that sleep on the server overlap rather than queue behind one another", async () => {
  const started = performance.now();
  const results = await Promise.all(
    Array.from({ length: 8 }, (_, index) => scalar("SELECT $1::int + 1 AS n FROM pg_sleep(0.5)", [index])),
  );
  assert.deepEqual(results, [1, 2, 3, 4, 5, 6, 7, 8]);
  assert.ok(performance.now() - started < 3000, "eight half-second queries ran concurrently");
});

// [spec:pgorm:req:napi.errors+1/test]
test("an error PostgreSQL reports rejects with a DatabaseError carrying its SQLSTATE", async () => {
  const error = await scalar("SELECT $1::int + 1 AS n", [2147483647]).then(
    () => assert.fail("the overflow resolved"),
    (error: unknown) => error,
  );
  assert.ok(error instanceof DatabaseError);
  assert.ok(error instanceof PgormError);
  assert.ok(error instanceof Error);
  assert.equal(error.name, "DatabaseError");
  assert.equal(error.sqlstate, "22003");
  assert.equal(error.severity, "ERROR");
  assert.equal(error.message, "integer out of range");
  assert.equal(error.detail, null);
});

// [spec:pgorm:req:napi.errors+1/test]
test("a statement PostgreSQL cannot parse rejects with its syntax-error SQLSTATE", async () => {
  await assert.rejects(pool.query("SELEC 1"), (error: unknown) => {
    assert.ok(error instanceof DatabaseError);
    assert.equal(error.sqlstate, "42601");
    return true;
  });
});

// [spec:pgorm:req:napi.errors+1/test]
test("an unreachable server rejects with a ConnectionError", async () => {
  await using unreachable = new Pool("postgres://pgorm@127.0.0.1:1/postgres?sslmode=disable");
  await assert.rejects(unreachable.query("SELECT 1"), (error: unknown) => {
    assert.ok(error instanceof ConnectionError);
    assert.ok(!("sqlstate" in error));
    return true;
  });
});

// [spec:pgorm:req:napi.errors+1/test]
test("a password never appears in the error its connection failure rejects with", async () => {
  await using unreachable = new Pool("postgres://pgorm:hunter2-secret@127.0.0.1:1/postgres?sslmode=disable");
  await assert.rejects(
    unreachable.query("SELECT 1"),
    (error: unknown) => {
      assert.ok(error instanceof ConnectionError);
      assert.ok(!String(error.message).includes("hunter2-secret"));
      return true;
    },
  );
});

// [spec:pgorm:req:napi.errors+1/test]
test("a parameter with no exact int4 rejects before anything is sent", async () => {
  for (const value of [1.5, 2 ** 31, -(2 ** 31) - 1, Number.NaN, Number.POSITIVE_INFINITY, "1"]) {
    await assert.rejects(scalar("SELECT $1::int + 1 AS n", [value]), (error: unknown) => {
      assert.ok(error instanceof ConstructionError, `${value}: ${error}`);
      assert.match(error.message, /parameter \$1/);
      return true;
    });
  }
});

// [spec:pgorm:req:napi.errors+1/test]
test("an unusable connection string is a ConstructionError", () => {
  assert.throws(() => new Pool("postgres://["), ConstructionError);
});

// [spec:pgorm:req:napi.errors+1/test]
test("a result the binding cannot decode exactly rejects with a DecodeError", async () => {
  await assert.rejects(pool.query("SELECT 'NaN'::numeric AS n"), DecodeError);
  await assert.rejects(pool.query("SELECT point(1, 2) AS n"), DecodeError);
});

// [spec:pgorm:req:napi.errors+1/test]
test("an argument of the wrong JavaScript type rejects with a TypeError", async () => {
  // deno-lint-ignore no-explicit-any
  await assert.rejects(pool.query(42 as any), TypeError);
  // deno-lint-ignore no-explicit-any
  await assert.rejects(pool.query("SELECT 1", [], { tagged: "yes" as any }), TypeError);
  // deno-lint-ignore no-explicit-any
  await assert.rejects(pool.query("SELECT 1", 1 as any), TypeError);
});
