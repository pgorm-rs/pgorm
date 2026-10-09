// SQL/JSON built from JavaScript against a live server, in either runtime:
// the query functions, constructors, IS JSON and JSON_TABLE, and the
// per-function behaviours that keep a choice PostgreSQL refuses from being
// written.

import assert from "node:assert/strict";
import { after, before, test } from "node:test";

import {
  bind,
  col,
  ConstructionError,
  DatabaseError,
  DataType,
  formatJson,
  isJson,
  isNotJson,
  jsonArray,
  jsonArrayAgg,
  jsonArrayQuery,
  jsonDefault,
  jsonExists,
  jsonObject,
  jsonObjectAgg,
  jsonParse,
  jsonQuery,
  jsonScalar,
  jsonSerialize,
  jsonTable,
  JsonTableColumn as C,
  jsonValue,
  Pool,
  select,
  Table,
  TypeName,
  Value,
} from "../lib/index.js";
import { scratchDatabase } from "./support.ts";

let database: { dsn: string; drop(): Promise<void> } | undefined;
let shared: Pool | undefined;

function pool(): Pool {
  if (!shared) throw new Error("the scratch database was not created");
  return shared;
}

const docs = new Table("docs", { alias: "d" });
const doc = docs.col("doc");

before(async () => {
  database = await scratchDatabase("pgorm_napi_json");
  shared = new Pool(database.dsn, { maxSize: 4 });
  for (
    const sql of [
      "CREATE TYPE mood AS ENUM ('calm', 'glad')",
      "CREATE TABLE docs (id int8 PRIMARY KEY, doc jsonb NOT NULL, body text)",
      `INSERT INTO docs VALUES
         (1, '{"size": 3, "price": 19.991, "tags": ["blue", "red"], "name": "one",
               "items": [{"n": 1, "name": "a", "flag": true, "parts": [1, 2]}, {"n": 2, "tags": ["x"]}, {"n": 0}]}',
          '{"name": "first"}'),
         (2, '{"size": "big", "tags": [], "items": []}', 'not json'),
         (3, '{"tags": ["green"], "items": [{"n": 5, "name": "calm"}]}', '[1, 2]')`,
    ]
  ) {
    await pool().execute(sql);
  }
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

async function column(expr: ReturnType<typeof col>, where = docs.col("id").eq(1)): Promise<unknown> {
  return (await pool().one(select(expr.as("v")).from(docs).where(where))).v;
}

// [spec:pgorm:req:napi.sql-json/test]
test("JSON_VALUE reads a scalar as the type it returns, with its DEFAULT and ERROR behaviours", async () => {
  assert.equal(await column(jsonValue(doc, "$.size", { returning: "integer" })), 3);
  assert.equal(
    String(await column(jsonValue(doc, "$.price", { returning: new DataType("numeric", { precision: 10, scale: 2 }) }))),
    "19.99",
  );
  assert.equal(await column(jsonValue(doc, "$.missing", { returning: "integer", onEmpty: jsonDefault(7) })), 7);
  assert.equal(
    await column(jsonValue(doc, "$.size", { returning: "integer", onError: jsonDefault(-1) }), docs.col("id").eq(2)),
    -1,
  );
  await assert.rejects(
    column(jsonValue(doc, "$.size", { returning: "integer", onError: "error" }), docs.col("id").eq(2)),
    (error: unknown) => error instanceof DatabaseError && error.sqlstate === "22P02",
  );
});

// [spec:pgorm:req:napi.sql-json/test]
test("JSON_EXISTS reads PASSING variables by their exact names, and JSON_QUERY shapes what it finds", async () => {
  const tagged = (tag: string) =>
    pool().query(select(docs.col("id")).from(docs).where(jsonExists(doc, "$.tags[*] ? (@ == $Tag)", { passing: { Tag: tag } })));
  assert.deepStrictEqual((await tagged("red")).map((row) => row.id), [1n]);
  assert.deepStrictEqual((await tagged("$Tag")).map((row) => row.id), []);
  assert.deepStrictEqual(await column(jsonQuery(doc, "$.tags[*]", { shaping: "withWrapper" })), ["blue", "red"]);
  assert.deepStrictEqual(
    await column(jsonQuery(doc, "$.nothing", { onEmpty: "emptyArray" })),
    [],
  );
  assert.equal(await column(jsonQuery(doc, "$.name", { returning: "text", shaping: "omitQuotes" })), "one");
  assert.equal(await column(jsonQuery(formatJson(docs.col("body")), "$.name", { returning: "text", shaping: "omitQuotes" })), "first");
});

// [spec:pgorm:req:napi.sql-json/test]
test("the constructors build JSON on the server, a bound number staying a number", async () => {
  const row = await pool().one(
    select(
      jsonObject({ id: docs.col("id"), body: formatJson(docs.col("body")), none: bind(Value.null("text")) }, { absentOnNull: true }).as("object"),
      jsonArray([1, "two", Value.json({ three: 3 })], { returning: "jsonb" }).as("array"),
      jsonScalar(5).as("scalar"),
      jsonParse(bind('{"a": [1, 2]}')).as("parsed"),
      jsonSerialize(doc, { returning: "text" }).as("text"),
      jsonArrayQuery(select(docs.col("id")).from(docs).orderBy(docs.col("id").desc())).as("ids"),
    ).from(docs).where(docs.col("id").eq(1)),
  );
  assert.deepStrictEqual(row.object, { id: 1, body: { name: "first" } });
  assert.deepStrictEqual(row.array, [1, "two", { three: 3 }]);
  assert.equal(row.scalar, 5);
  assert.deepStrictEqual(row.parsed, { a: [1, 2] });
  assert.equal(typeof row.text, "string");
  assert.deepStrictEqual(row.ids, [3, 2, 1]);
  await assert.rejects(
    pool().one(select(jsonObject([[bind("k"), 1], [bind("k"), 2]], { uniqueKeys: true }).as("v"))),
    (error: unknown) => error instanceof DatabaseError && error.sqlstate === "22030",
  );
});

// [spec:pgorm:req:napi.sql-json/test]
test("the JSON aggregates take their ordering, NULL handling and FILTER", async () => {
  const row = await pool().one(
    select(
      jsonArrayAgg(docs.col("id"), { orderBy: [docs.col("id").desc()], filter: docs.col("id").lt(3) }).as("ids"),
      jsonObjectAgg(docs.col("body"), docs.col("id"), { returning: "jsonb" }).as("bodies"),
    ).from(docs),
  );
  assert.deepStrictEqual(row.ids, [2, 1]);
  assert.deepStrictEqual(Object.keys(row.bodies as object).length, 3);
});

// [spec:pgorm:req:napi.sql-json/test]
test("IS JSON tests a value's kind", async () => {
  const rows = await pool().query(
    select(docs.col("id"), isJson(docs.col("body")).as("json"), isJson(docs.col("body"), { kind: "array" }).as("array"),
      isNotJson(docs.col("body"), { kind: "object", uniqueKeys: true }).as("notObject"))
      .from(docs).orderBy(docs.col("id").asc()),
  );
  assert.deepStrictEqual(rows.map((row) => [row.json, row.array, row.notObject]), [
    [true, false, false],
    [false, false, true],
    [true, true, true],
  ]);
});

// [spec:pgorm:req:napi.sql-json/test]
test("JSON_TABLE reads rows out of a document, each kind of column its own way, and LEFT JOIN keeps an empty one", async () => {
  const items = jsonTable(doc, "$.items[*] ? (@.n >= $Min)", [
    C.ordinality("i"),
    C.value("n", "integer"),
    C.value("label", "text", { path: "$.name", onEmpty: jsonDefault("none") }),
    C.query("tags", "jsonb", { onEmpty: "emptyArray" }),
    C.exists("flagged", "boolean", { path: "$.flag" }),
    C.nested("$.parts[*]", [C.value("part", "integer", { path: "$" })]),
  ], { alias: "jt", passing: { Min: 1 } });
  const rows = await pool().query(
    select(docs.col("id"), items.col("i"), items.col("n"), items.col("label"), items.col("tags"), items.col("flagged"), items.col("part"))
      .from(docs).from(items).orderBy(docs.col("id").asc(), items.col("i").asc(), items.col("part").asc()),
  );
  assert.deepStrictEqual(rows.map((row) => [row.id, row.i, row.n, row.label, row.tags, row.flagged, row.part]), [
    [1n, 1, 1, "a", [], true, 1],
    [1n, 1, 1, "a", [], true, 2],
    [1n, 2, 2, "none", ["x"], false, null],
    [3n, 1, 5, "calm", [], false, null],
  ]);
  const moods = jsonTable(doc, "$.items[*]", [C.value("mood", new DataType(new TypeName("mood")), { path: "$.name" })], {
    alias: "m",
  });
  const kept = await pool().query(
    select(docs.col("id"), moods.col("mood")).from(docs).join(moods, bind(true), { kind: "left" })
      .where(docs.col("id").gt(1)).orderBy(docs.col("id").asc()),
  );
  assert.deepStrictEqual(kept.map((row) => [row.id, row.mood]), [[2n, null], [3n, "calm"]]);
});

// [spec:pgorm:req:napi.sql-json/test]
test("a behaviour, type or shape a SQL/JSON function cannot take is refused as it is built", () => {
  assert.throws(() => jsonValue(doc, "$.a", { returning: "jsonb" }), refused(/JSON_VALUE cannot return json or jsonb/));
  assert.throws(() => jsonValue(doc, "$.a", { returning: "json" }), refused(/bug #19695/));
  assert.throws(() => jsonValue(doc, "$.a", { onEmpty: "emptyArray" as never }), refused(/JSON_VALUE's behaviour/));
  assert.throws(() => jsonExists(doc, "$.a", { onError: "null" as never }), refused(/JSON_EXISTS's behaviour/));
  assert.throws(() => jsonExists(doc, "$.a", { onError: jsonDefault(1) as never }), refused(/JSON_EXISTS's behaviour/));
  assert.throws(() => jsonQuery(doc, "$.a", { shaping: "wrapped" as never }), refused(/shaping/));
  assert.throws(() => jsonDefault(new Value("calm", new TypeName("mood"))), refused(/cannot carry an enum's/));
  assert.throws(() => jsonDefault(null as never), refused(/null has no kind/));
  assert.throws(() => jsonArrayAgg(col("v"), { orderBy: [col("v").asc({ nulls: "first" })] }), refused(/takes no NULLS FIRST/));
  assert.throws(() => jsonValue(doc, 1 as never), refused(/path is a string/));
  assert.throws(() => jsonExists(doc, "$.a", { passing: { "": 1 } }), refused(/identifier/));
  assert.throws(() => select(formatJson(col("x")) as never), refused(/not a FORMAT JSON input/));
  assert.throws(() => jsonTable(doc, "$", [] as never, { alias: "t" }), refused(/at least one column/));
  assert.throws(() => jsonTable(doc, "$", [C.ordinality("i")], {} as never), refused(/needs its alias/));
  assert.throws(() => jsonTable(doc, "$", [C.ordinality("i")], { alias: "t", onError: "null" as never }), refused(/JSON_TABLE's behaviour/));
  assert.throws(() => C.exists("e", "boolean", { onError: jsonDefault(true) as never }), refused(/EXISTS column's behaviour/));
  assert.throws(() => new DataType("integer", { length: 4 }), refused(/integer takes no length/));
  assert.throws(() => new DataType("numeric", { precision: 0 }), refused(/whole number of at least 1/));
  assert.throws(() => new DataType("text", { precision: 3 }), refused(/belong to numeric/));
  assert.throws(() => new DataType("varbit"), refused(/varbit takes a length/));
  assert.throws(() => new DataType("serial" as never), refused(/no built-in type/));
  assert.throws(() => new DataType(new TypeName("mood"), { length: 2 }), refused(/named type takes no size/));
});
