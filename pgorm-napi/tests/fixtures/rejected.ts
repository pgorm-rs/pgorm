// A query rejects, the script handles it, and has nothing left to do.

import process from "node:process";

import { DatabaseError, queryInt } from "../../lib/index.js";

try {
  await queryInt(process.env.PGORM_TEST_DSN ?? "", "SELECT $1::int + 1", [2147483647]);
} catch (error) {
  if (!(error instanceof DatabaseError)) throw error;
  console.log(`rejected ${error.sqlstate}`);
}
