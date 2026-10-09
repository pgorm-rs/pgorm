/**
 * pgorm from Node.js and Deno, through a Node-API addon.
 *
 * @module
 */

// [spec:pgorm:req:napi.typing]

/** The addon's version, which is the pgorm release it is built from. */
export declare const version: string;

/** The base class of every error pgorm-napi rejects with. */
export declare class PgormError extends Error {}

/** The server could not be reached, or the connection to it broke. */
export declare class ConnectionError extends PgormError {}

/** An argument could not become the value pgorm sends. */
export declare class ConstructionError extends PgormError {}

/** A result could not be decoded into the JavaScript value asked for. */
export declare class DecodeError extends PgormError {}

/**
 * pgorm or the binding failed in a way no input should cause, such as a panic
 * in its native code. Never an expected rejection of a caller's input.
 */
export declare class InternalError extends PgormError {}

/** The diagnostic fields of an error PostgreSQL reported. */
export interface DatabaseErrorDetails {
  /** The five-character SQLSTATE, e.g. `"22003"` for numeric_value_out_of_range. */
  readonly sqlstate: string;
  /** The severity the server reported, e.g. `"ERROR"` or `"FATAL"`. */
  readonly severity: string;
  readonly detail: string | null;
  readonly hint: string | null;
  readonly schema: string | null;
  readonly table: string | null;
  readonly column: string | null;
  readonly constraint: string | null;
}

/** PostgreSQL rejected the statement. */
export declare class DatabaseError extends PgormError implements DatabaseErrorDetails {
  constructor(message: string, details: DatabaseErrorDetails);
  readonly sqlstate: string;
  readonly severity: string;
  readonly detail: string | null;
  readonly hint: string | null;
  readonly schema: string | null;
  readonly table: string | null;
  readonly column: string | null;
  readonly constraint: string | null;
}

/**
 * Run `sql` on the pool for `dsn`, with each of `params` bound as an `int4`,
 * and resolve with the `int4` in the first column of the one row it returns.
 *
 * The pool for a connection string is opened on first use and shared by every
 * later call that names it.
 *
 * Rejects with a {@link DatabaseError} carrying the SQLSTATE when PostgreSQL
 * refuses the statement, a {@link ConnectionError} when the server cannot be
 * reached, a {@link ConstructionError} for an unusable connection string or a
 * parameter that is not a 32-bit integer, and a {@link DecodeError} when the
 * statement does not return exactly one row whose first column is an `int4`.
 */
export declare function queryInt(
  dsn: string,
  sql: string,
  params?: readonly number[],
): Promise<number>;
