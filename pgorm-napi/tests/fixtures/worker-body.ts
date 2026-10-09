// The worker side of worker.ts.

import { parentPort, workerData } from "node:worker_threads";

import { queryInt } from "../../lib/index.js";

const { dsn, mode } = workerData as { dsn: string; mode: string };

if (mode === "terminate") {
  queryInt(dsn, "SELECT $1::int + 1 FROM pg_sleep(2)", [1]).then(() => parentPort?.postMessage("settled"));
  parentPort?.postMessage("started");
} else {
  parentPort?.postMessage(`worker ${await queryInt(dsn, "SELECT $1::int + 1", [2])}`);
}
