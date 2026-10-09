// The script exits explicitly while queries are settling continuously, so
// runtime threads are sending settlements as the instance is torn down.

import process from "node:process";

import { query } from "../../lib/index.js";

const dsn = process.env.PGORM_TEST_DSN ?? "";
let settled = 0;

function next(): void {
  query(dsn, "SELECT $1::int + 1 AS n", [settled]).then(() => {
    settled += 1;
    next();
  });
}

for (let lane = 0; lane < 50; lane += 1) next();
setTimeout(() => {
  console.log(settled > 0 ? "racing" : "nothing settled");
  process.exit(0);
}, 300);
