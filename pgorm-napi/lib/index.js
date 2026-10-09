// @ts-check
// @ts-self-types="./index.d.ts"

// pgorm from Node.js and Deno. This module loads the native addon, hands it
// the error and value classes it rejects with and builds values from, and is
// what an application imports.
// [spec:pgorm:def:napi.api+1]
// [spec:pgorm:req:napi.typing]
//
// The addon is a Node-API module, loaded through CommonJS `require` because
// that is the one loader both runtimes give a `.node` file: Node's own, and in
// Deno the node:module compatibility layer's, which needs --allow-ffi to open
// a native library and --allow-read to resolve its path.

import { createRequire } from "node:module";

import { makeError } from "./errors.js";
import { install } from "./values.js";

// Dates and times are Temporal's, which Node.js carries from version 26.
// [spec:pgorm:req:napi.runtimes+1]
if (typeof globalThis.Temporal !== "object") {
  throw new Error(
    "pgorm-napi needs a runtime with Temporal as a global: Node.js 26 or later, or a Deno that carries it",
  );
}

// [spec:pgorm:req:napi.loading]
const require = createRequire(import.meta.url);
const native = require("./pgorm_napi.node");

native.setErrorFactory(makeError);
install(native);

export {
  ConnectionError,
  ConstructionError,
  DatabaseError,
  DecodeError,
  InternalError,
  PgormError,
} from "./errors.js";
export {
  CreatedMultirange,
  CreatedRange,
  Decimal,
  Interval,
  Multirange,
  Range,
  TypeName,
  Uuid,
  Value,
} from "./values.js";

/** @type {string} */
export const version = native.version;

/**
 * Each row an object keyed by column name, in column order. Built with
 * `Object.fromEntries`, which defines its keys, so a column named
 * `__proto__` is a property like any other rather than the object's
 * prototype.
 * [spec:pgorm:req:napi.rows]
 *
 * @param {string[]} names
 * @param {unknown[][]} rows
 */
function objects(names, rows) {
  return rows.map((values) => Object.fromEntries(names.map((name, index) => [name, values[index]])));
}

// An async function, so an argument the addon refuses synchronously rejects
// the promise rather than throwing at the call site.
// [spec:pgorm:req:napi.promises]
/**
 * @param {string} dsn
 * @param {string} sql
 * @param {readonly unknown[]} [params]
 * @param {{ tagged?: boolean }} [options]
 * @returns {Promise<any[]>}
 */
export async function query(dsn, sql, params = [], options = {}) {
  const tagged = options.tagged ?? false;
  if (typeof tagged !== "boolean") throw new TypeError("options.tagged is a boolean");
  const [names, rows] = await native.query(dsn, sql, params, tagged);
  return objects(names, rows);
}
