// Build an application's own native module — the binding's whole API and the
// entities the application registers, in one — and hold it to its suite in
// both runtimes, as pgorm-python's checks/entities.py does for its wheel.
//
//   node pgorm-napi/checks/entities.js
//
// 1. Run the application crate's Rust tests: its registry and the parity of
//    its entities' statements with tests/parity.json.
// 2. Build its cdylib.
// 3. Materialize the binding's module beside it, under target/napi-entities,
//    with the application's library as lib/pgorm_napi.node: the facade an
//    application ships with its own native module.
// 4. Run the application's JavaScript suite against it under node --test and
//    deno test, and write target/napi-entities/report.json.
//
// The suite reads PGORM_TEST_DSN, or the server DATABASE_URL names, as the
// binding's own suite does, and makes and drops a database of its own.
// [spec:pgorm:req:napi.application]

import { spawnSync } from "node:child_process";
import { copyFileSync, cpSync, mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { dsn } from "../tests/support.ts";

const napi = dirname(dirname(fileURLToPath(import.meta.url)));
const checkout = dirname(napi);
const application = join(napi, "tests", "application-binding");
const manifest = join(application, "Cargo.toml");
const target = process.env.CARGO_TARGET_DIR ?? join(checkout, "target");
const out = join(target, "napi-entities");
const facade = join(out, "pgorm-napi");

/**
 * Run a command to completion, its output passed through; a failure ends
 * the check with the command's status.
 *
 * @param {string} command
 * @param {string[]} args
 * @param {{ cwd?: string, env?: Record<string, string | undefined>, capture?: boolean }} [options]
 */
function run(command, args, { cwd = checkout, env = process.env, capture = false } = {}) {
  console.error(`$ ${command} ${args.join(" ")}`);
  const result = spawnSync(command, args, {
    cwd,
    env,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    stdio: capture ? ["ignore", "pipe", "inherit"] : "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    console.error(`${command} exited with ${result.status}`);
    process.exit(result.status ?? 1);
  }
  return result.stdout ?? "";
}

run("cargo", ["test", "--manifest-path", manifest, "--locked", "--target-dir", target]);

const built = run("cargo", [
  "build",
  "--manifest-path",
  manifest,
  "--locked",
  "--lib",
  "--target-dir",
  target,
  "--message-format=json-render-diagnostics",
], { capture: true });
const library = built
  .split("\n")
  .filter((line) => line.startsWith("{"))
  .map((line) => JSON.parse(line))
  .filter((message) =>
    message.reason === "compiler-artifact" && message.target?.name === "application" &&
    message.target.crate_types.includes("cdylib")
  )
  .flatMap((message) => message.filenames)
  .find((file) => /\.(dylib|so|dll)$/.test(file));
if (!library) {
  console.error("cargo reported no application cdylib");
  process.exit(1);
}

rmSync(out, { recursive: true, force: true });
mkdirSync(join(facade, "lib"), { recursive: true });
for (const file of readdirSync(join(napi, "lib"))) {
  if (/\.(js|d\.ts)$/.test(file)) copyFileSync(join(napi, "lib", file), join(facade, "lib", file));
}
copyFileSync(library, join(facade, "lib", "pgorm_napi.node"));
copyFileSync(join(napi, "package.json"), join(facade, "package.json"));
writeFileSync(join(facade, "deno.json"), `${JSON.stringify({ nodeModulesDir: "none" }, null, 2)}\n`);
mkdirSync(join(facade, "tests", "parity"), { recursive: true });
copyFileSync(join(napi, "tests", "support.ts"), join(facade, "tests", "support.ts"));
copyFileSync(join(napi, "tests", "parity", "canonical.ts"), join(facade, "tests", "parity", "canonical.ts"));
cpSync(join(application, "tests"), join(facade, "tests", "application-binding", "tests"), { recursive: true });

const env = { ...process.env, PGORM_TEST_DSN: dsn(), NO_COLOR: "1" };
const suite = join(facade, "tests", "application-binding", "tests");
const files = readdirSync(suite).filter((file) => file.endsWith(".test.ts")).map((file) => join(suite, file));
run(process.execPath, ["--test", ...files], { cwd: facade, env });
run("deno", ["test", "--allow-ffi", "--allow-read", "--allow-env", "--allow-run", "--config", join(facade, "deno.json"), ...files], {
  cwd: facade,
  env,
});

writeFileSync(
  join(out, "report.json"),
  `${JSON.stringify({ library, facade, suites: files.map((file) => file.slice(suite.length + 1)), runtimes: ["node", "deno"] }, null, 2)}\n`,
);
console.error(`the application module passed its suite in both runtimes: ${join(out, "report.json")}`);
