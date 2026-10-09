// The worker side of worker.ts.

import { parentPort, workerData } from "node:worker_threads";

import { Pool } from "../../lib/index.js";

const { dsn, mode } = workerData as { dsn: string; mode: string };
const pool = new Pool(dsn);

if (mode === "terminate") {
  pool.query("SELECT $1::int + 1 AS n FROM pg_sleep(2)", [1]).then(() => parentPort?.postMessage("settled"));
  parentPort?.postMessage("started");
} else {
  parentPort?.postMessage(`worker ${(await pool.query("SELECT $1::int + 1 AS n", [2]))[0]?.n}`);
}
