// A bound query through pgorm's pool settles a promise, in either runtime.
//
// The suite uses the public API through its declarations, which `deno test`
// and `deno check` type-check before anything runs.
// [spec:pgorm:req:napi.typing/test]

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  ConnectionError,
  ConstructionError,
  DatabaseError,
  DecodeError,
  PgormError,
  queryInt,
  version,
} from "../lib/index.js";
import { dsn } from "./support.ts";

// [spec:pgorm:req:napi.loading/test]
test("the addon loads and reports the pgorm release it was built from", () => {
  assert.match(version, /^\d+\.\d+\.\d+/);
});

// [spec:pgorm:req:napi.promises/test]
test("a bound query resolves with its result", async () => {
  assert.equal(await queryInt(dsn(), "SELECT $1::int + 1", [41]), 42);
});

// [spec:pgorm:req:napi.promises/test]
test("a hundred queries in flight at once each resolve with their own result", async () => {
  const values = Array.from({ length: 100 }, (_, index) => index * 1000 - 50_000);
  const pending = values.map((value) => queryInt(dsn(), "SELECT $1::int + 1", [value]));
  assert.deepEqual(await Promise.all(pending), values.map((value) => value + 1));
});

// [spec:pgorm:req:napi.promises/test]
test("queries that sleep on the server overlap rather than queue behind one another", async () => {
  const started = performance.now();
  const results = await Promise.all(
    Array.from({ length: 8 }, (_, index) => queryInt(dsn(), "SELECT $1::int + 1 FROM pg_sleep(0.5)", [index])),
  );
  assert.deepEqual(results, [1, 2, 3, 4, 5, 6, 7, 8]);
  assert.ok(performance.now() - started < 3000, "eight half-second queries ran concurrently");
});

// [spec:pgorm:req:napi.errors/test]
test("an error PostgreSQL reports rejects with a DatabaseError carrying its SQLSTATE", async () => {
  const error = await queryInt(dsn(), "SELECT $1::int + 1", [2147483647]).then(
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

// [spec:pgorm:req:napi.errors/test]
test("a statement PostgreSQL cannot parse rejects with its syntax-error SQLSTATE", async () => {
  await assert.rejects(queryInt(dsn(), "SELEC 1", []), (error: unknown) => {
    assert.ok(error instanceof DatabaseError);
    assert.equal(error.sqlstate, "42601");
    return true;
  });
});

// [spec:pgorm:req:napi.errors/test]
test("an unreachable server rejects with a ConnectionError", async () => {
  await assert.rejects(queryInt("postgres://pgorm@127.0.0.1:1/postgres", "SELECT 1", []), (error: unknown) => {
    assert.ok(error instanceof ConnectionError);
    assert.ok(!("sqlstate" in error));
    return true;
  });
});

// [spec:pgorm:req:napi.errors/test]
test("a password never appears in the error its connection failure rejects with", async () => {
  await assert.rejects(
    queryInt("postgres://pgorm:hunter2-secret@127.0.0.1:1/postgres", "SELECT 1", []),
    (error: unknown) => {
      assert.ok(error instanceof ConnectionError);
      assert.ok(!String(error.message).includes("hunter2-secret"));
      return true;
    },
  );
});

// [spec:pgorm:req:napi.errors/test]
test("a parameter with no exact int4 rejects before anything is sent", async () => {
  for (const value of [1.5, 2 ** 31, -(2 ** 31) - 1, Number.NaN, Number.POSITIVE_INFINITY]) {
    await assert.rejects(queryInt(dsn(), "SELECT $1::int + 1", [value]), ConstructionError);
  }
  // deno-lint-ignore no-explicit-any
  await assert.rejects(queryInt(dsn(), "SELECT $1::int + 1", ["1" as any]), ConstructionError);
});

// [spec:pgorm:req:napi.errors/test]
test("an unusable connection string rejects with a ConstructionError", async () => {
  await assert.rejects(queryInt("postgres://[", "SELECT 1", []), ConstructionError);
});

// [spec:pgorm:req:napi.errors/test]
test("a result that is not one int4 row rejects with a DecodeError", async () => {
  await assert.rejects(queryInt(dsn(), "SELECT 'text'", []), DecodeError);
  await assert.rejects(queryInt(dsn(), "SELECT 1 WHERE false", []), DecodeError);
});

// [spec:pgorm:req:napi.errors/test]
test("an argument of the wrong JavaScript type rejects with a TypeError", async () => {
  // deno-lint-ignore no-explicit-any
  await assert.rejects(queryInt(42 as any, "SELECT 1", []), TypeError);
});
