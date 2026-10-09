// Generate or verify the npm packages' Rust dependency notices, as
// pgorm-python's checks/notices.py does for its wheel.
//
//   node pgorm-napi/checks/notices.js           # verify
//   node pgorm-napi/checks/notices.js --write   # regenerate after a dependency change
//
// The inventory is the committed lockfile's whole Cargo graph — build and
// platform-conditional dependencies included, so a superset of what any one
// platform's addon links — read from Cargo's exact package metadata and the
// notice files each package ships. `licenses/supplemental.json` adds the
// upstream notices some crate archives omit, each held to its recorded hash.
// The two generated files, notices/DEPENDENCIES.json and
// notices/THIRD_PARTY_NOTICES.txt, are committed, and every platform package
// carries them beside its addon.
// [spec:pgorm:req:napi.notices]

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, relative, sep } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const napi = dirname(dirname(fileURLToPath(import.meta.url)));
const checkout = dirname(napi);

/** Where the generated files live, which the packaging check copies. */
export const NOTICES = join(napi, "notices");

/** @param {Uint8Array} data */
function digest(data) {
  return createHash("sha256").update(data).digest("hex");
}

const NOTICE_PREFIXES = ["license", "licence", "copying", "copyright", "notice", "unlicense"];

/**
 * Every file under `directory`, recursively.
 *
 * @param {string} directory
 * @returns {string[]}
 */
function walk(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return walk(path);
    return entry.isFile() ? [path] : [];
  });
}

/**
 * The notice files a package ships: a registry package's whole archive is
 * searched, a local crate's own directory only, since local crates share the
 * repository's licences and their directories hold build outputs.
 *
 * @param {{ manifest_path: string, source: string | null }} pkg
 */
function noticePaths(pkg) {
  const directory = dirname(pkg.manifest_path);
  const candidates = pkg.source
    ? walk(directory)
    : readdirSync(directory).map((name) => join(directory, name)).filter((path) => statSync(path).isFile());
  const found = candidates.filter((path) => {
    const name = basename(path);
    const lower = name.toLowerCase();
    return NOTICE_PREFIXES.some((prefix) => lower.startsWith(prefix)) || name === "AUTHORS" ||
      relative(directory, path).split(sep).some((part) => ["licenses", "licences"].includes(part.toLowerCase()));
  });
  if (found.length === 0 && !pkg.source) {
    found.push(join(checkout, "LICENSE-APACHE"), join(checkout, "LICENSE-MIT"));
  }
  return found.sort((a, b) => {
    const left = relative(directory, a).split(sep);
    const right = relative(directory, b).split(sep);
    for (let index = 0; index < Math.min(left.length, right.length); index += 1) {
      if (left[index] !== right[index]) return /** @type {string} */ (left[index]) < /** @type {string} */ (right[index]) ? -1 : 1;
    }
    return left.length - right.length;
  });
}

/**
 * The bundled C components the inventory names beside the Rust packages,
 * keyed by the pg_query release that vendors libpg_query. A lockfile moving
 * pg_query to a release not listed here fails, so the libpg_query revision
 * and the PostgreSQL parser version are reviewed with it.
 */
const PG_QUERY_COMPONENTS = {
  "18.0.0": { libpg_query: "c1546e7e97edc93fe8474d7a59881e91fbf48cfc", postgres: "18.6" },
};

/**
 * @param {any} pkg
 * @param {Record<string, { file: string, source: string, sha256: string }[]>} supplements
 * @returns {[string, Uint8Array][]}
 */
function textsFor(pkg, supplements) {
  const directory = dirname(pkg.manifest_path);
  /** @type {[string, Uint8Array][]} */
  const texts = noticePaths(pkg).map((path) => {
    const inside = !relative(directory, path).startsWith("..");
    // The path inside the package, so a crate vendoring several projects'
    // LICENSE files names which is which.
    return [inside ? relative(directory, path).split(sep).join("/") : basename(path), readFileSync(path)];
  });
  for (const entry of supplements[`${pkg.name}@${pkg.version}`] ?? []) {
    const data = readFileSync(join(napi, "licenses", entry.file));
    if (digest(data) !== entry.sha256) throw new Error(`supplemental licence hash differs: ${entry.file}`);
    texts.push([entry.source, data]);
  }
  if (pkg.name === "pg_query") {
    // xxHash carries its licence only in its header; upb and utf8_range ship
    // LICENSE files the search above already found.
    const header = readFileSync(join(directory, "libpg_query", "vendor", "xxhash", "xxhash.h"));
    const end = header.indexOf("*/");
    if (end < 0) throw new Error("xxhash.h has no licence comment");
    texts.push(["libpg_query/vendor/xxhash/xxhash.h", Buffer.concat([header.subarray(0, end + 2), Buffer.from("\n")])]);
  }
  if (texts.length === 0 || !pkg.license) {
    throw new Error(`missing licence evidence for ${pkg.name} ${pkg.version}`);
  }
  return texts;
}

/**
 * The two generated files' contents, from the locked Cargo graph.
 *
 * @returns {{ manifest: string, notices: string }}
 */
export function generate() {
  const metadata = JSON.parse(
    execFileSync(
      "cargo",
      ["metadata", "--manifest-path", join(napi, "Cargo.toml"), "--format-version", "1", "--locked"],
      { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 },
    ),
  );
  const supplements = JSON.parse(readFileSync(join(napi, "licenses", "supplemental.json"), "utf8"));
  const decoder = new TextDecoder("utf-8", { fatal: true });
  /** @type {Map<string, string>} */
  const notices = new Map();
  /** @type {any[]} */
  const sorted = [...metadata.packages].sort((a, b) =>
    a.name === b.name ? (a.version < b.version ? -1 : a.version > b.version ? 1 : 0) : a.name < b.name ? -1 : 1
  );
  const packages = sorted.map((pkg) => {
    const references = textsFor(pkg, supplements).map(([source, data]) => {
      const identity = digest(data);
      notices.set(identity, decoder.decode(data));
      return { source, sha256: identity };
    });
    return {
      name: pkg.name,
      version: pkg.version,
      license: pkg.license,
      repository: pkg.repository,
      source: pkg.source ?? "pgorm workspace",
      notices: references,
    };
  });
  /** @param {string} name */
  const owner = (name) => {
    const found = sorted.find((pkg) => pkg.name === name);
    if (!found) throw new Error(`the locked graph has no ${name}`);
    return `${name}@${found.version}`;
  };
  const pgQuery = owner("pg_query");
  const vendored = PG_QUERY_COMPONENTS[/** @type {keyof typeof PG_QUERY_COMPONENTS} */ (pgQuery.slice("pg_query@".length))];
  if (!vendored) {
    throw new Error(`${pgQuery} is not in PG_QUERY_COMPONENTS: record the libpg_query revision and PostgreSQL parser it vendors`);
  }
  const components = [
    { name: "libpg_query", revision: vendored.libpg_query, owner: pgQuery, license: "BSD-3-Clause" },
    { name: "PostgreSQL parser", version: vendored.postgres, owner: pgQuery, license: "PostgreSQL" },
    { name: "upb", owner: pgQuery, license: "BSD-3-Clause" },
    { name: "utf8_range", owner: pgQuery, license: "MIT" },
    { name: "xxHash", owner: pgQuery, license: "BSD-2-Clause" },
    {
      name: "ring native cryptography (including BoringSSL and fiat code)",
      owner: owner("ring"),
      license: "see ring notices",
    },
  ];
  const report = {
    schema_version: 1,
    scope:
      "Locked Cargo graph, including build and platform-conditional dependencies; a superset of any one platform's addon",
    cargo_lock_sha256: digest(readFileSync(join(napi, "Cargo.lock"))),
    javascript_runtime_dependencies: [],
    packages,
    bundled_native_components: components,
  };
  const { name } = JSON.parse(readFileSync(join(napi, "package.json"), "utf8"));
  let text = `${name} npm packages — third-party notices\n\n`;
  text += "The companion DEPENDENCIES.json maps packages and bundled native components to the notice hashes below.\n\n";
  for (const identity of [...notices.keys()].sort()) {
    text += `===== SHA-256 ${identity} =====\n${notices.get(identity)}\n\n`;
  }
  return { manifest: `${JSON.stringify(report, null, 2)}\n`, notices: text };
}

if (import.meta.url === new URL(process.argv[1] ?? "", "file:").href) {
  const write = process.argv.includes("--write");
  const { manifest, notices } = generate();
  for (const [name, content] of /** @type {[string, string][]} */ ([
    ["DEPENDENCIES.json", manifest],
    ["THIRD_PARTY_NOTICES.txt", notices],
  ])) {
    const path = join(NOTICES, name);
    if (write) {
      writeFileSync(path, content);
    } else if (!existsSync(path) || readFileSync(path, "utf8") !== content) {
      console.error(`${name} is stale; run node pgorm-napi/checks/notices.js --write`);
      process.exit(1);
    }
  }
  console.error("Verified locked dependency metadata and native component notices.");
}
