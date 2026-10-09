// @ts-check
// @ts-self-types="./index.d.ts"

// pgorm from Node.js and Deno. This module loads the native addon, defines the
// error classes it rejects with, and is what an application imports.
// [spec:pgorm:def:napi.api]
// [spec:pgorm:req:napi.typing]
//
// The addon is a Node-API module, loaded through CommonJS `require` because
// that is the one loader both runtimes give a `.node` file: Node's own, and in
// Deno the node:module compatibility layer's, which needs --allow-ffi to open
// a native library and --allow-read to resolve its path.

import { createRequire } from "node:module";

// [spec:pgorm:req:napi.loading]
const require = createRequire(import.meta.url);
const native = require("./pgorm_napi.node");

// [spec:pgorm:req:napi.errors]
/** The base class of every error pgorm-napi rejects with. */
export class PgormError extends Error {}

/** The server could not be reached, or the connection to it broke. */
export class ConnectionError extends PgormError {}

/** An argument could not become the value pgorm sends. */
export class ConstructionError extends PgormError {}

/** A result could not be decoded into the JavaScript value asked for. */
export class DecodeError extends PgormError {}

/** pgorm or the binding failed in a way no input should cause. */
export class InternalError extends PgormError {}

/** PostgreSQL rejected the statement. */
export class DatabaseError extends PgormError {
  /**
   * @param {string} message
   * @param {import("./index.d.ts").DatabaseErrorDetails} details
   */
  constructor(message, details) {
    super(message);
    this.sqlstate = details.sqlstate;
    this.severity = details.severity;
    this.detail = details.detail;
    this.hint = details.hint;
    this.schema = details.schema;
    this.table = details.table;
    this.column = details.column;
    this.constraint = details.constraint;
  }
}

for (const type of [PgormError, ConnectionError, ConstructionError, DecodeError, InternalError, DatabaseError]) {
  Object.defineProperty(type.prototype, "name", {
    value: type.name,
    writable: true,
    configurable: true,
  });
}

/** @type {Record<string, new (message: string, details: any) => PgormError>} */
const classes = { ConnectionError, ConstructionError, DecodeError, InternalError, DatabaseError };

native.setErrorFactory(
  /**
   * @param {string} kind
   * @param {string} message
   * @param {any} details
   */
  (kind, message, details) => new (classes[kind] ?? PgormError)(message, details),
);

/** @type {string} */
export const version = native.version;

// An async function, so an argument the addon refuses synchronously rejects
// the promise rather than throwing at the call site.
// [spec:pgorm:req:napi.promises]
/**
 * @param {string} dsn
 * @param {string} sql
 * @param {readonly number[]} [params]
 * @returns {Promise<number>}
 */
export async function queryInt(dsn, sql, params = []) {
  return await native.queryInt(dsn, sql, params);
}
