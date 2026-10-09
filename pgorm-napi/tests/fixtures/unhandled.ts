// A query rejects with nothing to handle it: the runtime reports the
// unhandled rejection and exits with a failure status.

import process from "node:process";

import { queryInt } from "../../lib/index.js";

queryInt(process.env.PGORM_TEST_DSN ?? "", "SELECT $1::int + 1", [2147483647]);
