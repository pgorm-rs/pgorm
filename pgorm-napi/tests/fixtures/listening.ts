// A listener nothing references any more is collected, which closes its
// connection and so ends its session; one still open when the script ends
// holds nothing the event loop waits on, so the process exits all the same.
// Run with the garbage collector exposed.

import process from "node:process";

import { Pool } from "../../lib/index.js";

const collect = (globalThis as { gc?: () => void }).gc;
if (collect === undefined) throw new Error("the garbage collector is not exposed");

const pool = new Pool(process.env.PGORM_TEST_DSN ?? "", { maxSize: 1 });
const channel = `napi_collected_${process.pid}`;

async function listening(): Promise<number> {
  const row = await pool.one("SELECT count(*)::int AS n FROM pg_stat_activity WHERE query = $1", [
    `LISTEN "${channel}"`,
  ]);
  return row.n as number;
}

await (async () => {
  const listener = await pool.listener();
  await listener.listen(channel);
})();
console.log(`listening ${await listening()}`);

// Each round trip yields to the event loop, which runs the finalizers a
// collection queued.
let remaining = await listening();
for (let attempt = 0; attempt < 500 && remaining !== 0; attempt += 1) {
  collect();
  await new Promise((resolve) => setImmediate(resolve));
  remaining = await listening();
}
console.log(`collected ${remaining}`);

const held = await pool.listener();
await held.listen(`${channel}_held`);
console.log("holding");
