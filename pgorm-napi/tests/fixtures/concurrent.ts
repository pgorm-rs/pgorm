// A hundred queries in flight at once, then nothing left to do.

import process from "node:process";

import { queryInt } from "../../lib/index.js";

const dsn = process.env.PGORM_TEST_DSN ?? "";
const results = await Promise.all(
  Array.from({ length: 100 }, (_, index) => queryInt(dsn, "SELECT $1::int + 1", [index])),
);
console.log(`settled ${results.reduce((sum, value) => sum + value, 0)}`);
