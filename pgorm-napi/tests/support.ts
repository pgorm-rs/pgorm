// Shared by the test files, which run unchanged under `node --test` and
// `deno test`: both runners accept node:test's `test`, and every runtime
// difference the suite meets is kept here.
// [spec:pgorm:req:napi.runtimes+1/test]

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));

/** The repository checkout this package sits in. */
const checkout = join(here, "..", "..");

/** Whether the suite is running under Deno rather than Node. */
export const deno = "Deno" in globalThis;

function fromEnvFile(name: string): string | undefined {
  for (const file of [".env.local", ".env"]) {
    const path = join(checkout, file);
    if (!existsSync(path)) continue;
    for (const line of readFileSync(path, "utf8").split("\n")) {
      const match = /^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$/.exec(line);
      if (match?.[1] === name) return match[2]?.replace(/^(['"])(.*)\1$/, "$2");
    }
  }
  return undefined;
}

/**
 * The connection string the live tests use: `PGORM_TEST_DSN` if it is set,
 * otherwise the server `DATABASE_URL` names — from the environment, or from
 * the checkout's `.env.local` or `.env` as the Rust suite reads it — and its
 * `postgres` maintenance database, which the queries here leave untouched.
 */
export function dsn(): string {
  const explicit = process.env.PGORM_TEST_DSN;
  if (explicit) return explicit;
  const server = process.env.DATABASE_URL ?? fromEnvFile("DATABASE_URL");
  if (!server) {
    throw new Error(
      "set PGORM_TEST_DSN, or DATABASE_URL to a PostgreSQL server URL, to run the live suite",
    );
  }
  const url = new URL(server);
  if (url.pathname === "" || url.pathname === "/") url.pathname = "/postgres";
  return url.toString();
}

/** What a child process did. */
export interface Outcome {
  readonly code: number | null;
  readonly signal: string | null;
  readonly stdout: string;
  readonly stderr: string;
  /** Wall-clock milliseconds from spawn to exit. */
  readonly elapsed: number;
  /** Whether it outlived its deadline and was killed. */
  readonly hung: boolean;
}

/**
 * Run `tests/fixtures/<name>.ts` in a fresh process of the runtime running
 * the suite, with the permissions a native addon needs under Deno, and kill
 * it if it has not exited by `deadline` milliseconds.
 */
export function runFixture(name: string, deadline: number, env: Record<string, string> = {}): Promise<Outcome> {
  const script = join(here, "fixtures", `${name}.ts`);
  const args = deno
    ? ["run", "--allow-ffi", "--allow-read", "--allow-env", script]
    : [script];
  const started = performance.now();
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, args, {
      env: { ...process.env, PGORM_TEST_DSN: dsn(), NO_COLOR: "1", ...env },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    let hung = false;
    child.stdout.setEncoding("utf8").on("data", (chunk: string) => (stdout += chunk));
    child.stderr.setEncoding("utf8").on("data", (chunk: string) => (stderr += chunk));
    const timer = setTimeout(() => {
      hung = true;
      child.kill("SIGKILL");
    }, deadline);
    child.on("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.on("close", (code, signal) => {
      clearTimeout(timer);
      resolve({ code, signal, stdout, stderr, elapsed: performance.now() - started, hung });
    });
  });
}

/**
 * A database of its own for a test file, named for the runtime and process so
 * the two runtimes' suites can run at once, made fresh on the server
 * {@link dsn} names and dropped by `drop`.
 */
export async function scratchDatabase(prefix: string): Promise<{ dsn: string; drop(): Promise<void> }> {
  const { query } = await import("../lib/index.js");
  const name = `${prefix}_${deno ? "deno" : "node"}_${process.pid}`;
  const server = dsn();
  await query(server, `DROP DATABASE IF EXISTS ${name} WITH (FORCE)`);
  await query(server, `CREATE DATABASE ${name}`);
  const url = new URL(server);
  url.pathname = `/${name}`;
  return {
    dsn: url.toString(),
    async drop() {
      await query(server, `DROP DATABASE IF EXISTS ${name} WITH (FORCE)`);
    },
  };
}

/**
 * Assert two plain values equal, comparing what `assert.deepStrictEqual`
 * cannot see: a Temporal value has no own properties, so any two of a class
 * would pass it, and the module's `Decimal` and `Uuid` keep their text in a
 * private field. Each is compared by class and text, recursively through
 * arrays, ranges and plain objects.
 */
export function same(actual: unknown, expected: unknown, path = "value"): void {
  if (typeof expected !== "object" || expected === null) {
    assert.deepStrictEqual(actual, expected, path);
    return;
  }
  assert.equal(
    Object.getPrototypeOf(actual),
    Object.getPrototypeOf(expected),
    `${path}: ${actual?.constructor?.name} is not ${expected.constructor?.name}`,
  );
  if (expected instanceof Uint8Array || expected instanceof Float32Array) {
    assert.deepStrictEqual(actual, expected, path);
  } else if (Array.isArray(expected)) {
    assert.ok(Array.isArray(actual), path);
    assert.equal(actual.length, expected.length, `${path}.length`);
    expected.forEach((item, index) => same(actual[index], item, `${path}[${index}]`));
  } else if (Object.getOwnPropertyNames(expected).length === 0) {
    assert.equal(String(actual), String(expected), path);
  } else {
    assert.deepStrictEqual(Object.keys(actual as object), Object.keys(expected), `${path} keys`);
    for (const [key, item] of Object.entries(expected)) {
      same((actual as Record<string, unknown>)[key], item, `${path}.${key}`);
    }
  }
}
