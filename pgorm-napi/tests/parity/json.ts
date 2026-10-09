// The SQL/JSON family's parity cases — query functions, constructors, IS JSON
// and JSON_TABLE — held to `json.json` as `select.ts`'s are to theirs.
// [spec:pgorm:req:napi.sql-json/test]

import {
  bind,
  type Builder,
  col,
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
  select,
  Table,
  TypeName,
  Value,
} from "../../lib/index.js";

const doc = col("doc");

export const cases: Record<string, () => Builder> = {
  "json-value": () =>
    jsonValue(doc, "$.size", { returning: "integer", onEmpty: jsonDefault(0), onError: "error" }),
  "json-value-sized": () =>
    jsonValue(doc, "$.price", { returning: new DataType("numeric", { precision: 10, scale: 2 }), onEmpty: "null" }),
  "json-exists-passing": () => jsonExists(doc, "$.tags[*] ? (@ == $Tag)", { passing: { Tag: "blue" }, onError: "false" }),
  "json-query": () =>
    jsonQuery(doc, "$.tags[*]", { returning: "jsonb", shaping: "withWrapper", onEmpty: "emptyArray", onError: "null" }),
  "json-query-omit": () =>
    jsonQuery(formatJson(col("body")), "$.name", { returning: new DataType("varchar", { length: 20 }), shaping: "omitQuotes" }),
  "json-object": () =>
    jsonObject({ id: col("id"), body: formatJson(col("body")) }, { absentOnNull: true, returning: "jsonb" }),
  "json-object-pairs": () => jsonObject([[col("k"), 1], [col("k"), 2]], { uniqueKeys: true }),
  "json-array": () => jsonArray([1, "two", formatJson(col("three"))], { nullOnNull: true, returning: "json" }),
  "json-array-query": () => jsonArrayQuery(select(col("id")).from(new Table("t")), { returning: "jsonb" }),
  "json-aggregates": () =>
    select(
      jsonObjectAgg(col("k"), col("v"), { absentOnNull: true, uniqueKeys: true, returning: "jsonb", filter: col("v").isNotNull() }),
      jsonArrayAgg(col("v"), { orderBy: [col("k").desc()], nullOnNull: true, returning: "jsonb", filter: col("k").gt(0) }),
    ).from(new Table("t")),
  "json-parse-scalar-serialize": () =>
    select(
      jsonParse(bind('{"a": 1}'), { uniqueKeys: true }),
      jsonScalar(5),
      jsonScalar(bind(new Value(5, "i16"))),
      jsonSerialize(doc, { returning: "text" }),
    ),
  "is-json": () =>
    select(
      isJson(doc),
      isJson(doc, { kind: "object", uniqueKeys: true }),
      isNotJson(col("body"), { kind: "array" }),
      isJson(bind("[1]"), { kind: "scalar" }),
    ),
  "json-table": () => {
    const docs = new Table("docs", { alias: "d" });
    const items = jsonTable(docs.col("doc"), "$.items[*] ? (@.n >= $Min)", [
      C.ordinality("i"),
      C.value("n", "integer"),
      C.value("label", "text", { path: "$.name", onEmpty: jsonDefault("none"), onError: "null" }),
      C.query("tags", "jsonb", { onEmpty: "emptyArray", shaping: "withConditionalWrapper" }),
      C.exists("flagged", "boolean", { path: "$.flag", onError: "false" }),
      C.nested("$.parts[*]", [C.value("part", "text", { path: "$" })], { pathName: "parts" }),
    ], { alias: "jt", passing: { Min: 1 }, pathName: "root", onError: "empty" });
    return select(docs.col("id"), items.col("n"), items.col("part")).from(docs).from(items);
  },
  "json-table-left-join": () => {
    const docs = new Table("docs", { alias: "d" });
    const labels = jsonTable(docs.col("doc"), "$.items[*]", [C.value("label", new DataType(new TypeName("mood")))], { alias: "l" });
    return select(docs.col("id"), labels.star()).from(docs).join(labels, bind(true), { kind: "left" });
  },
};
