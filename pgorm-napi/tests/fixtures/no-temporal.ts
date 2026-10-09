// A runtime without Temporal: loading the module throws, naming what it needs.

Reflect.deleteProperty(globalThis, "Temporal");
try {
  await import("../../lib/index.js");
  console.log("loaded");
} catch (error) {
  console.log(error instanceof Error ? error.message : String(error));
}
