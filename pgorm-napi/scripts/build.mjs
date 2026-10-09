// Build the addon and place it where lib/index.js loads it.
//
//   node scripts/build.mjs [--release]
//
// The crate is an rlib for the applications that link it, so the addon's
// cdylib is asked of `cargo rustc`. Cargo names a cdylib for the platform
// (libpgorm_napi.dylib, .so, pgorm_napi.dll); Node-API hosts load it from any
// path, so it is copied to lib/pgorm_napi.node. Builds go to the repository's
// target directory, which the nplan checks share, unless CARGO_TARGET_DIR
// says otherwise.

import { spawnSync } from "node:child_process";
import { copyFileSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const release = process.argv.includes("--release");
const args = [
  "rustc",
  "--manifest-path",
  join(root, "Cargo.toml"),
  "--locked",
  "--lib",
  "--crate-type",
  "cdylib",
  "--message-format=json-render-diagnostics",
];
if (release) args.push("--release");
if (!process.env.CARGO_TARGET_DIR) {
  args.push("--target-dir", join(root, "..", "target"));
}

const cargo = spawnSync("cargo", args, {
  encoding: "utf8",
  maxBuffer: 64 * 1024 * 1024,
  stdio: ["ignore", "pipe", "inherit"],
});
if (cargo.error) throw cargo.error;
if (cargo.status !== 0) process.exit(cargo.status ?? 1);

const library = cargo.stdout
  .split("\n")
  .filter((line) => line.startsWith("{"))
  .map((line) => JSON.parse(line))
  .filter((message) =>
    message.reason === "compiler-artifact" &&
    message.target?.name === "pgorm_napi" &&
    message.target.crate_types.includes("cdylib")
  )
  .flatMap((message) => message.filenames)
  .find((file) => /\.(dylib|so|dll)$/.test(file));
if (!library) {
  console.error("cargo reported no pgorm_napi cdylib");
  process.exit(1);
}

// Replaced rather than overwritten: macOS kills a process that maps a signed
// binary whose file changed in place, and a fresh inode sidesteps it.
const destination = join(root, "lib", "pgorm_napi.node");
rmSync(destination, { force: true });
copyFileSync(library, destination);
console.error(`${library} -> ${destination}`);
