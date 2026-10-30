// Parity: the schema `fromEntity` gives for each entity the application
// registers is the one `schema-parity.json` holds — the file the application
// crate's Rust test writes with pgorm's `Schema` and its entities directly —
// and a table pgorm refuses is refused with its message, in both runtimes.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  ConstructionError,
  CreateIndex,
  CreateTable,
  CreateType,
  type Ddl,
  entities,
  entity,
  EntitySchema,
  fromEntity,
} from "../../../lib/index.js";

type Generated =
  | { readonly table: string; readonly enums: string[]; readonly indexes: string[]; readonly comments: string[] }
  | { readonly refused: string };

const golden = JSON.parse(readFileSync(new URL("./schema-parity.json", import.meta.url), "utf8")) as Record<
  string,
  Generated
>;

/** A DDL statement's SQL, which carries no values beside it. */
function sql(statement: Ddl): string {
  const { sql, values } = statement.inspect();
  assert.deepStrictEqual(values, []);
  return sql;
}

// [spec:pgorm:req:bindings.schema-from-entity/test]
test("every registered entity's schema is in the golden file", () => {
  assert.deepStrictEqual(entities(), Object.keys(golden).sort());
});

for (const name of entities()) {
  // [spec:pgorm:req:bindings.schema-from-entity/test]
  test(`schema: ${name} is what pgorm's Schema writes`, () => {
    const expected = golden[name];
    assert.ok(expected, `${name} is in the golden file`);
    if ("refused" in expected) {
      assert.throws(() => fromEntity(entity(name)), (error: unknown) => {
        assert.ok(error instanceof ConstructionError, `expected a ConstructionError, got ${error}`);
        assert.equal(error.message, expected.refused);
        return true;
      });
      return;
    }
    const generated = fromEntity(entity(name));
    assert.ok(generated instanceof EntitySchema);
    assert.ok(generated.table instanceof CreateTable);
    assert.ok(generated.enums.every((statement) => statement instanceof CreateType));
    assert.ok(generated.indexes.every((statement) => statement instanceof CreateIndex));
    assert.deepStrictEqual(
      {
        table: sql(generated.table),
        enums: generated.enums.map(sql),
        indexes: generated.indexes.map(sql),
        comments: generated.comments.map(sql),
      },
      expected,
    );
  });
}
