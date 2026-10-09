// @ts-check

// The error classes every failure rejects or throws with.
// [spec:pgorm:req:napi.errors+1]

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

/** A pool, connection, transaction or stream was used after it closed, or while another operation held it. */
export class LifecycleError extends PgormError {}

/** No connection became free within the pool's acquire budget. */
export class TimeoutError extends PgormError {}

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

for (
  const type of [
    PgormError,
    ConnectionError,
    ConstructionError,
    DecodeError,
    InternalError,
    LifecycleError,
    TimeoutError,
    DatabaseError,
  ]
) {
  Object.defineProperty(type.prototype, "name", {
    value: type.name,
    writable: true,
    configurable: true,
  });
}

/** @type {Record<string, new (message: string, details: any) => PgormError>} */
const classes = {
  ConnectionError,
  ConstructionError,
  DecodeError,
  InternalError,
  LifecycleError,
  TimeoutError,
  DatabaseError,
};

/**
 * Build the error a failure of `kind` rejects or throws with.
 *
 * @param {string} kind
 * @param {string} message
 * @param {any} details
 */
export function makeError(kind, message, details) {
  return new (classes[kind] ?? PgormError)(message, details);
}
