// Ends while it still holds everything it opened: a connection with a
// transaction open on it, and a stream with rows left on another. Their
// connections live on the runtime, which keeps nothing waiting, so the
// process exits all the same.

import process from "node:process";

import { Pool } from "../../lib/index.js";

const pool = new Pool(process.env.PGORM_TEST_DSN ?? "", { maxSize: 2 });
const connection = await pool.acquire();
const transaction = await connection.begin();
await transaction.execute("SELECT 1");
const stream = pool.stream("SELECT generate_series(1, 1000) AS n");
const first = await stream.next();
console.log(`holding ${first.value?.n} ${transaction.closed}`);
