// Awaits one query, then has nothing left to do: its pool, still open, holds
// idle connections on the runtime, which keep nothing waiting.

import process from "node:process";

import { Pool } from "../../lib/index.js";

const pool = new Pool(process.env.PGORM_TEST_DSN ?? "");

const [row] = await pool.query("SELECT $1::int + 1 AS n", [1]);
console.log(`settled ${row?.n}`);
