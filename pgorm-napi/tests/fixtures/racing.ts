// The script exits explicitly while queries are settling continuously, so
// runtime threads are sending settlements as the instance is torn down. It
// exits from the settlement that brings the last of its lanes into the
// stream, with every other lane's next query in flight. A fixed delay cannot
// stand in for that condition: a loaded machine or a slow server may have
// settled nothing by the time it runs out.

import process from "node:process";

import { Pool } from "../../lib/index.js";

const LANES = 50;

const dsn = process.env.PGORM_TEST_DSN ?? "";
const pool = new Pool(dsn);
let settled = 0;
let unstarted = LANES;

function next(first: boolean): void {
  pool.query("SELECT $1::int + 1 AS n", [settled]).then(() => {
    settled += 1;
    if (first && --unstarted === 0) {
      console.log("racing");
      process.exit(0);
    }
    next(false);
  });
}

for (let lane = 0; lane < LANES; lane += 1) next(true);
