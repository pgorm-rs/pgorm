// A query rejects, the script handles it, and has nothing left to do.

import process from "node:process";

import { DatabaseError, Pool } from "../../lib/index.js";

const pool = new Pool(process.env.PGORM_TEST_DSN ?? "");

try {
  await pool.query("SELECT $1::int + 1 AS n", [2147483647]);
} catch (error) {
  if (!(error instanceof DatabaseError)) throw error;
  console.log(`rejected ${error.sqlstate}`);
}
