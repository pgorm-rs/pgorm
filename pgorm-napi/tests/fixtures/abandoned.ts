// The script exits explicitly while a query is still running on the server.
// The runtime's threads are still busy with it; the exit must neither wait
// for them nor crash as the instance is torn down underneath them.

import process from "node:process";

import { Pool } from "../../lib/index.js";

const pool = new Pool(process.env.PGORM_TEST_DSN ?? "");

pool.query("SELECT $1::int + 1 AS n FROM pg_sleep(10)", [1]).then(() =>
  console.log("settled")
);
setTimeout(() => {
  console.log("exiting");
  process.exit(0);
}, 250);
