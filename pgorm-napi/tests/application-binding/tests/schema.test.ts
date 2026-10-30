// A registered entity's generated schema, run against a live server in
// either runtime: the account's enum, table, index and comments create what
// the entity reads and writes, the note's foreign key holds, and the
// temper's key, which PostgreSQL cannot build, is refused rather than left
// out.

import assert from "node:assert/strict";
import { after, before, test } from "node:test";

import {
  ConstructionError,
  DatabaseError,
  entity,
  ForeignKeyViolationError,
  fromEntity,
  Pool,
} from "../../../lib/index.js";
import { scratchDatabase } from "../../support.ts";

let database: { dsn: string; drop(): Promise<void> } | undefined;
let shared: Pool | undefined;

function pool(): Pool {
  if (!shared) throw new Error("the scratch database was not created");
  return shared;
}

before(async () => {
  database = await scratchDatabase("pgorm_napi_entity_schema");
  shared = new Pool(database.dsn, { maxSize: 2 });
});

after(async () => {
  await shared?.close();
  await database?.drop();
});

// [spec:pgorm:req:bindings.schema-from-entity/test]
test("a registered entity's generated schema creates the table its records are read from and written to", async () => {
  const Account = entity("app.Account");
  const Note = entity("app.Note");
  const generated = fromEntity(Account);
  assert.ok(Object.isFrozen(generated) && Object.isFrozen(generated.enums));
  assert.deepStrictEqual([generated.enums.length, generated.indexes.length, generated.comments.length], [1, 1, 2]);
  assert.notStrictEqual(fromEntity(Account).enums, generated.enums);
  await pool().execute("CREATE SCHEMA napi_entities");
  try {
    for (const statement of generated.enums) await pool().execute(statement);
    await pool().execute(generated.table);
    for (const statement of [...generated.indexes, ...generated.comments]) await pool().execute(statement);
    const notes = fromEntity(Note);
    await pool().execute(notes.table);

    const inserted = await Account.active().set("id", 1).set("display name", "Native").set("note", null).insert(pool());
    assert.equal(inserted["display name"], "Native|before");
    assert.equal(inserted.mood, "calm");
    assert.equal((await Account.find().one(pool())).id, 1);
    await Note.active().set("id", 10).set("account_id", 1).set("body", "kept").insert(pool());
    await assert.rejects(
      Note.active().set("id", 11).set("account_id", 2).set("body", "orphan").insert(pool()),
      ForeignKeyViolationError,
    );

    const comments = await pool().one(
      `SELECT obj_description('napi_entities.accounts'::regclass, 'pg_class') AS table_comment,
              col_description('napi_entities.accounts'::regclass, 2) AS column_comment`,
    );
    assert.deepStrictEqual(
      { ...comments },
      { table_comment: "Accounts' native schema", column_comment: "The user's display name" },
    );
    const indexes = await pool().query(
      "SELECT indexname FROM pg_indexes WHERE schemaname = 'napi_entities' AND tablename = 'accounts' ORDER BY indexname",
    );
    assert.deepStrictEqual(indexes.map((row) => row.indexname), ["accounts_pkey", "idx-accounts-display name"]);
  } finally {
    await pool().execute("DROP SCHEMA napi_entities CASCADE");
  }
});

// [spec:pgorm:req:bindings.schema-from-entity/test]
test("a foreign key PostgreSQL cannot build is refused with pgorm's message, and no schema leaves it out", async () => {
  let refusal: unknown;
  assert.throws(() => fromEntity(entity("app.Temper")), (error: unknown) => {
    refusal = error;
    return error instanceof ConstructionError;
  });
  for (const part of ["Settles", "settles_into", "42804"]) assert.match(String(refusal), new RegExp(part));
  assert.throws(() => fromEntity("app.Temper" as never), TypeError);

  // The key refused is one the server cannot build either, written by hand.
  await pool().execute("CREATE SCHEMA napi_entities");
  try {
    for (
      const sql of [
        "CREATE TYPE napi_entities.mood AS ENUM ('calm', 'busy')",
        "CREATE DOMAIN napi_entities.temper AS napi_entities.mood",
        "CREATE TABLE napi_entities.tempers (mood napi_entities.temper PRIMARY KEY, settles_into napi_entities.temper NOT NULL)",
      ]
    ) {
      await pool().execute(sql);
    }
    await assert.rejects(
      pool().execute(
        "ALTER TABLE napi_entities.tempers ADD FOREIGN KEY (settles_into) REFERENCES napi_entities.tempers (mood)",
      ),
      (error: unknown) => error instanceof DatabaseError && error.sqlstate === "42804",
    );
  } finally {
    await pool().execute("DROP SCHEMA napi_entities CASCADE");
  }
});
