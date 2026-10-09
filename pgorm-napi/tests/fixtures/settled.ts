// Awaits one query, then has nothing left to do.

import process from "node:process";

import { queryInt } from "../../lib/index.js";

console.log(`settled ${await queryInt(process.env.PGORM_TEST_DSN ?? "", "SELECT $1::int + 1", [1])}`);
