// A hundred queries in flight at once, then nothing left to do.

import process from "node:process";

import { Pool } from "../../lib/index.js";

const dsn = process.env.PGORM_TEST_DSN ?? "";
const pool = new Pool(dsn);
const results = await Promise.all(
  Array.from({ length: 100 }, (_, index) => pool.query("SELECT $1::int + 1 AS n", [index])),
);
console.log(`settled ${results.reduce((sum, [row]) => sum + Number(row?.n), 0)}`);
