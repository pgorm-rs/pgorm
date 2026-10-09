// The script's last statement runs while a query is still in flight; the
// process has to stay up until the query settles, then exit.

import process from "node:process";

import { query } from "../../lib/index.js";

query(process.env.PGORM_TEST_DSN ?? "", "SELECT $1::int + 1 AS n FROM pg_sleep(1)", [1]).then(([row]) =>
  console.log(`settled ${row?.n}`)
);
console.log("script ended");
