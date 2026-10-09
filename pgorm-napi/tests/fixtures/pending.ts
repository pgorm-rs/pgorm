// The script's last statement runs while a query is still in flight; the
// process has to stay up until the query settles, then exit.

import process from "node:process";

import { queryInt } from "../../lib/index.js";

queryInt(process.env.PGORM_TEST_DSN ?? "", "SELECT $1::int + 1 FROM pg_sleep(1)", [1]).then((value) =>
  console.log(`settled ${value}`)
);
console.log("script ended");
