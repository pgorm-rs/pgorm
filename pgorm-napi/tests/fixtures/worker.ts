// A worker thread loads its own instance of the addon beside the main
// thread's. WORKER_MODE says how the worker ends: `finish`, it runs a query
// and has nothing left to do; `terminate`, the main thread terminates it while
// its query is still running on the server, then keeps the process up with a
// longer query of its own, so the runtime thread finishing the worker's query
// tries to settle it into the torn-down instance while the process is alive.

import process from "node:process";
import { Worker } from "node:worker_threads";

import { queryInt } from "../../lib/index.js";

const dsn = process.env.PGORM_TEST_DSN ?? "";
const mode = process.env.WORKER_MODE ?? "finish";

console.log(`main ${await queryInt(dsn, "SELECT $1::int + 1", [1])}`);

const worker = new Worker(new URL("./worker-body.ts", import.meta.url), { workerData: { dsn, mode } });
worker.on("message", (message: string) => {
  console.log(message);
  if (message === "started") void worker.terminate();
});
worker.on("error", (error) => {
  console.error(error);
  process.exitCode = 1;
});
worker.on("exit", async () => {
  console.log("worker exited");
  if (mode === "terminate") {
    console.log(`main ${await queryInt(dsn, "SELECT $1::int + 1 FROM pg_sleep(3)", [3])}`);
  }
});
