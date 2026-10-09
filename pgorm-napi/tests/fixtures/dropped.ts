// Promises left to Neon's drop queue: their Deferreds are dropped unsettled on
// a runtime thread, and the queue, a threadsafe function the event loop does
// not wait on, rejects them on the JavaScript thread. DROP_MODE `held` keeps
// the loop alive with a timer until all ten have been rejected, then releases
// it; `unheld` holds nothing, so the process may exit before the queue
// delivers — Node.js does, Deno delivers first — and reports how many it saw.

import { createRequire } from "node:module";
import process from "node:process";

const native = createRequire(import.meta.url)("../../lib/pgorm_napi.node") as {
  probeDropQueue?: () => Promise<undefined>;
};

let rejected = 0;
const probes = Array.from({ length: 10 }, () =>
  native.probeDropQueue?.().catch(() => {
    rejected += 1;
  })
);

if (native.probeDropQueue === undefined) {
  console.log("release build");
} else if (process.env.DROP_MODE === "held") {
  const hold = setInterval(() => {}, 60_000);
  await Promise.all(probes);
  clearInterval(hold);
  console.log(`rejected ${rejected}`);
} else {
  process.on("exit", () => console.log(`rejected ${rejected}`));
}
