// Awaits one query, then has nothing left to do.

import process from "node:process";

import { query } from "../../lib/index.js";

const [row] = await query(process.env.PGORM_TEST_DSN ?? "", "SELECT $1::int + 1 AS n", [1]);
console.log(`settled ${row?.n}`);
