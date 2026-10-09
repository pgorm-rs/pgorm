// A query rejects with nothing to handle it: the runtime reports the
// unhandled rejection and exits with a failure status.

import process from "node:process";

import { Pool } from "../../lib/index.js";

const pool = new Pool(process.env.PGORM_TEST_DSN ?? "");

pool.query("SELECT $1::int + 1 AS n", [2147483647]);
