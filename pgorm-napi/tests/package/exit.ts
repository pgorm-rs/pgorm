// Run by the smoke suite in a fresh process: imports the installed package,
// awaits one query on a pool it leaves open, and has nothing left to do.

import process from "node:process";

import { Pool } from "@necessary/pgorm";

const pool = new Pool(process.env.PGORM_TEST_DSN ?? "");
const row = await pool.one("SELECT $1::int4 + 1 AS n", [41]);
console.log(`settled ${row.n}`);
