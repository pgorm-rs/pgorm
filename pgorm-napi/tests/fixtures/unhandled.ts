// A query rejects with nothing to handle it: the runtime reports the
// unhandled rejection and exits with a failure status.

import process from "node:process";

import { query } from "../../lib/index.js";

query(process.env.PGORM_TEST_DSN ?? "", "SELECT $1::int + 1 AS n", [2147483647]);
