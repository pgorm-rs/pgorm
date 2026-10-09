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

/**
 * A pool, connection, transaction or stream was used after it closed, or
 * while another operation held it: a second statement on a busy connection, a
 * parent transaction used while a savepoint of it is open.
 */
export declare class LifecycleError extends PgormError {}

/** No connection became free within the pool's `acquireTimeout`. */
export declare class TimeoutError extends PgormError {}

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

// [spec:pgorm:req:napi.values]

/** The built-in range kinds, named as PostgreSQL names the types. */
export type RangeKind = "int4range" | "int8range" | "numrange" | "daterange" | "tsrange" | "tstzrange";

/** The built-in multirange kinds. */
export type MultirangeKind =
  | "int4multirange"
  | "int8multirange"
  | "nummultirange"
  | "datemultirange"
  | "tsmultirange"
  | "tstzmultirange";

/**
 * A value kind with no type name, spelled as pgorm-python spells it. Each is
 * one of pgorm's value variants, and `interval` the one PostgreSQL type pgorm
 * has no variant for.
 *
 * | Kind | PostgreSQL | JavaScript |
 * | --- | --- | --- |
 * | `bool` | `boolean` | `boolean` |
 * | `i8`, `i16`, `i32`, `u32` | `"char"`, `int2`, `int4`, `oid` | `number`, an integer in range |
 * | `i64`, `u64` | `int8` | `bigint` (or a safe-integer `number`, in) |
 * | `f32`, `f64` | `float4`, `float8` | `number` (`f32` only when exact) |
 * | `text`, `char` | `text`, `varchar`, `bpchar`, `name` | `string` (`char`: one code point) |
 * | `bytes` | `bytea` | `Uint8Array` |
 * | `json` | `json`, `jsonb` | {@link JsonValue} |
 * | `decimal` | `numeric` | {@link Decimal} |
 * | `uuid` | `uuid` | {@link Uuid} |
 * | `date` | `date` | `Temporal.PlainDate` |
 * | `time` | `time` | `Temporal.PlainTime` |
 * | `datetime` | `timestamp` | `Temporal.PlainDateTime` |
 * | `datetime_utc` | `timestamptz` | `Temporal.Instant` |
 * | `interval` | `interval` | {@link Interval} (or a `Temporal.Duration`, in) |
 * | `ipnetwork` | `inet`, `cidr` | `string`, address and prefix |
 * | `mac_address` | `macaddr` | `Uint8Array` of six bytes |
 * | `vector` | pgvector's `vector` | `Float32Array` |
 * | a {@link RangeKind} or {@link MultirangeKind} | that type | {@link Range}, {@link Multirange} |
 */
export type ScalarKind =
  | "bool"
  | "i8"
  | "i16"
  | "i32"
  | "i64"
  | "u32"
  | "u64"
  | "f32"
  | "f64"
  | "text"
  | "char"
  | "bytes"
  | "json"
  | "decimal"
  | "uuid"
  | "date"
  | "time"
  | "datetime"
  | "datetime_utc"
  | "interval"
  | "ipnetwork"
  | "mac_address"
  | "vector"
  | RangeKind
  | MultirangeKind;

/** A kind: a scalar kind's name, an enum's {@link TypeName}, or a created range or multirange type. */
export type Kind = ScalarKind | TypeName | CreatedRange | CreatedMultirange;

/**
 * A JSON document as JavaScript. An integer literal past
 * `Number.MAX_SAFE_INTEGER` reads as a `bigint`, exactly; a number that has
 * no exact binary form is a {@link DecodeError}.
 */
export type JsonValue =
  | null
  | boolean
  | number
  | bigint
  | string
  | readonly JsonValue[]
  | { readonly [key: string]: JsonValue };

/** A value as a row or {@link Value.value} gives it: the JavaScript type of its kind, or `null` for SQL NULL. */
export type PlainValue =
  | null
  | boolean
  | number
  | bigint
  | string
  | Uint8Array
  | Float32Array
  | Decimal
  | Uuid
  | Interval
  | Temporal.PlainDate
  | Temporal.PlainTime
  | Temporal.PlainDateTime
  | Temporal.Instant
  | Range
  | Multirange
  | JsonValue
  | readonly PlainValue[];

// [spec:pgorm:req:napi.inference]
/**
 * A statement parameter: a {@link Value}, or a plain value whose kind is
 * inferred. `null` is SQL NULL of no declared kind; a number is an `i64` when
 * it is a safe integer and an `f64` otherwise; a bigint is an `i64`; a string
 * `text`; a `Uint8Array` `bytes`; a plain object `json`; a Temporal value or
 * one of the module's classes its kind; an array an array of the one kind its
 * items infer as. A range, an empty array and `undefined` cannot be inferred
 * and are refused with a {@link ConstructionError}.
 */
export type Param =
  | Value
  | null
  | boolean
  | number
  | bigint
  | string
  | Uint8Array
  | Decimal
  | Uuid
  | Interval
  | Temporal.PlainDate
  | Temporal.PlainTime
  | Temporal.PlainDateTime
  | Temporal.Instant
  | Temporal.Duration
  | { readonly [key: string]: JsonValue }
  | readonly Param[];

// [spec:pgorm:req:napi.rows]
/** A row: its columns' values by column name, in column order. */
export type Row = { [column: string]: PlainValue };

/** A row read with `{ tagged: true }`: each column's value with its kind. */
export type TaggedRow = { [column: string]: Value };

/** A PostgreSQL type named by identifier, optionally schema-qualified: an enum's type. */
export declare class TypeName {
  constructor(name: string, options?: { schema?: string | null });
  readonly name: string;
  readonly schema: string | null;
  /** The name quoted, as SQL spells it: `"app"."mood"`. */
  toString(): string;
}

/** The subtypes a created range type can range over. */
export type CreatedSubtype =
  | "i16"
  | "i32"
  | "i64"
  | "f32"
  | "f64"
  | "decimal"
  | "text"
  | "date"
  | "time"
  | "datetime"
  | "datetime_utc"
  | "uuid";

/**
 * A range type a schema created with `CREATE TYPE ... AS RANGE`, and the kind
 * of its subtype, which converts each bound. pgorm holds a value of one as its
 * text form, so a statement reads it from text and casts it:
 * `CAST($1::text AS measure.floatrange)`.
 */
export declare class CreatedRange {
  constructor(name: string, subtype: CreatedSubtype, options?: { schema?: string | null });
  readonly name: string;
  readonly subtype: CreatedSubtype;
  readonly schema: string | null;
}

/** The multirange type PostgreSQL creates beside a created range type, named by its own name. */
export declare class CreatedMultirange {
  constructor(name: string, subtype: CreatedSubtype, options?: { schema?: string | null });
  readonly name: string;
  readonly subtype: CreatedSubtype;
  readonly schema: string | null;
}

/**
 * An exact decimal: PostgreSQL's `numeric` within pgorm's range, a 96-bit
 * coefficient and at most 28 fractional digits. Made from a string in plain
 * notation or a bigint, never a number; its text keeps its scale (`"19.9900"`).
 * It does not become a number implicitly, which could round it.
 */
export declare class Decimal {
  constructor(value: string | bigint);
  toString(): string;
  toJSON(): string;
}

/** A UUID, held as its lower-case hyphenated text. */
export declare class Uuid {
  constructor(value: string);
  equals(other: unknown): boolean;
  toString(): string;
  toJSON(): string;
}

/**
 * PostgreSQL's `interval`: months, days and microseconds, each with its own
 * sign, because a month has no fixed number of days nor a day of hours. A
 * `Temporal.Duration`'s fields share one sign, so it cannot hold
 * `1 mon -2 days`; this can.
 */
export declare class Interval {
  constructor(fields?: { months?: number; days?: number; microseconds?: bigint | number });
  readonly months: number;
  readonly days: number;
  readonly microseconds: bigint;
  /**
   * From a `Temporal.Duration` or an ISO 8601 duration: years and months
   * become months, weeks and days days, the rest microseconds. A nanosecond
   * digit is a {@link ConstructionError}, never truncated.
   */
  static from(value: Interval | Temporal.Duration | string): Interval;
  /** The `Temporal.Duration` of the same fields; a `RangeError` when their signs differ. */
  toDuration(): Temporal.Duration;
  /** PostgreSQL's `iso_8601` interval style, each field signed: `P1M-2DT3H`. */
  toString(): string;
  toJSON(): string;
}

/**
 * A range: the empty range, or the values between two bounds, `null` on a
 * side being no bound. A side with no bound includes nothing, so its bracket
 * is always `(` or `)`. Its kind is not inferred from its bounds: bind one as
 * `new Value(range, "int4range")`, or with a {@link CreatedRange}.
 */
export declare class Range<T = PlainValue> {
  constructor(lower?: T | null, upper?: T | null, bounds?: "[)" | "[]" | "(]" | "()");
  /** The range containing no value, unlike `new Range()`, which contains every value. */
  static empty<T = PlainValue>(): Range<T>;
  readonly lower: T | null;
  readonly upper: T | null;
  readonly lowerInclusive: boolean;
  readonly upperInclusive: boolean;
  readonly isEmpty: boolean;
  readonly bounds: "[)" | "[]" | "(]" | "()";
  toString(): string;
}

/** A multirange: a set of ranges of one subtype. */
export declare class Multirange<T = PlainValue> implements Iterable<Range<T>> {
  constructor(ranges?: Iterable<Range<T>>);
  readonly ranges: readonly Range<T>[];
  readonly length: number;
  [Symbol.iterator](): Iterator<Range<T>>;
  toString(): string;
}

// [spec:pgorm:req:napi.value-tags]
/**
 * A value with its kind declared: what plain JavaScript cannot tell apart —
 * an integer's width, a typed SQL NULL, JSON's null, an empty array, an enum's
 * type, a range's type — and any other kind written out explicitly.
 *
 * Construction converts at once, and throws a {@link ConstructionError} for
 * data the kind cannot hold exactly. A value is immutable and can be bound
 * any number of times.
 */
export declare class Value {
  /** A value of `kind`, or, with no kind, of the kind `data` infers as. */
  constructor(data: unknown, kind?: Kind);
  /** SQL NULL of a kind. */
  static null(kind: Kind): Value;
  /** A JSON document; `null` here is JSON's null, not SQL NULL. */
  static json(data: JsonValue): Value;
  /**
   * An array of a declared element kind, the empty array included; `items`
   * `null` is an SQL NULL array that still names its element kind.
   */
  static array(kind: Kind, items: readonly unknown[] | null): Value;
  /** The kind's name: a {@link ScalarKind}, `enum`, `created_range`, `created_multirange` or `array`. */
  readonly kind: string;
  readonly isNull: boolean;
  /** An enum's (or an enum array's) type, or a created range's. */
  readonly typeName: TypeName | null;
  /** A created range's or multirange's type. */
  readonly createdType: CreatedRange | CreatedMultirange | null;
  /** An array's element kind. */
  readonly elementType: Kind | null;
  /** The plain JavaScript value, independently owned. */
  readonly value: PlainValue;
  /** An array's items, each a value of the element kind; `null` for an SQL NULL array. */
  items(): Value[] | null;
  /** The same kind and the same value; floats compare by their bits. */
  equals(other: unknown): boolean;
  toString(): string;
}

// [spec:pgorm:req:napi.connections]
/** How a {@link Pool} connects. Durations are in milliseconds. */
export interface PoolOptions {
  /**
   * `"verify-full"`, the default unless the connection string says
   * `sslmode=disable`, verifies the server's certificate and host name and
   * never falls back to plaintext; `"disable"` connects in plaintext.
   */
  readonly tls?: "verify-full" | "disable";
  /** PEM certificates to trust instead of the platform's trust store. */
  readonly ca?: string | Uint8Array;
  /** Connections the pool keeps at most; 10 by default. */
  readonly maxSize?: number;
  /** How long opening a connection may take; 10 000 by default. */
  readonly connectTimeout?: number;
  /** How long `acquire` waits for a free connection before a {@link TimeoutError}; 30 000 by default. */
  readonly acquireTimeout?: number;
  /** Prepared statements each connection caches; 0 disables the cache; 128 by default. */
  readonly statementCacheSize?: number;
  /** How a returned connection is checked before reuse: `"verified"` (the default) or `"fast"`. */
  readonly recycle?: "verified" | "fast";
}

/** A snapshot of a pool's capacity and use. */
export interface PoolStatus {
  readonly maxSize: number;
  readonly size: number;
  readonly available: number;
  readonly waiting: number;
}

// [spec:pgorm:req:napi.cancellation]
/** Options of the operations that wait on the server or the pool: an `AbortSignal` that ends one. */
export interface OperationOptions {
  /**
   * Aborting rejects with the signal's reason. The operation's outcome is
   * then unknown — a write may or may not have happened — so its connection
   * is discarded, and a transaction it ran in ends.
   */
  readonly signal?: AbortSignal;
}

// [spec:pgorm:req:napi.results]
/** Options of the row terminals. */
export interface QueryOptions extends OperationOptions {
  /** Give each column as a {@link Value} carrying its kind. */
  readonly tagged?: boolean;
}

/** The row a terminal gives for its options. */
export type RowFor<O extends QueryOptions> = O extends { readonly tagged: true } ? TaggedRow : Row;

// [spec:pgorm:req:napi.transactions]
/** How a transaction opens; a savepoint inherits its transaction's. */
export interface TransactionOptions extends OperationOptions {
  /**
   * `"default"` inherits the session's; `"readWrite"` and `"readOnly"` set the
   * access mode; `"deferrable"` is `SERIALIZABLE READ ONLY DEFERRABLE`.
   */
  readonly mode?: "default" | "readWrite" | "readOnly" | "deferrable";
  /** An isolation level, with `"readWrite"` or `"readOnly"` only. */
  readonly isolation?: "readUncommitted" | "readCommitted" | "repeatableRead" | "serializable" | null;
}

/**
 * What a pool, a connection and a transaction run bound SQL through. A
 * parameter is bound, never interpolated.
 */
export declare abstract class Queryable {
  /** Run a statement and resolve with the number of rows it affected. */
  execute(sql: string, params?: readonly Param[], options?: OperationOptions): Promise<number>;
  /** Every row. */
  query<O extends QueryOptions = QueryOptions>(sql: string, params?: readonly Param[], options?: O): Promise<RowFor<O>[]>;
  /** Exactly one row; any other count is a {@link DecodeError}. */
  one<O extends QueryOptions = QueryOptions>(sql: string, params?: readonly Param[], options?: O): Promise<RowFor<O>>;
  /** At most one row, or `null`; more is a {@link DecodeError}. */
  optional<O extends QueryOptions = QueryOptions>(
    sql: string,
    params?: readonly Param[],
    options?: O,
  ): Promise<RowFor<O> | null>;
}

/**
 * A pool of connections to one server, opened as they are first needed: the
 * constructor sends nothing, and throws a {@link ConstructionError} for an
 * unusable connection string or option. Its statements each run on a
 * connection of their own. Close it to release its connections; it is also
 * `await using`-disposable.
 */
export declare class Pool extends Queryable implements AsyncDisposable {
  constructor(dsn: string, options?: PoolOptions);
  readonly closed: boolean;
  status(): PoolStatus;
  /** A connection of the caller's own until it closes it. */
  acquire(options?: OperationOptions): Promise<Connection>;
  /** Run `use` with a connection, closed when it settles. */
  connection<T>(use: (connection: Connection) => Promise<T>, options?: OperationOptions): Promise<T>;
  /** Run `use` in a transaction on a connection of its own: committed when it resolves, rolled back when it throws. */
  transaction<T>(use: (transaction: Transaction) => Promise<T>, options?: TransactionOptions): Promise<T>;
  /** The rows of `sql`, one per pull, over a connection the stream holds until it ends. */
  stream<O extends QueryOptions = QueryOptions>(sql: string, params?: readonly Param[], options?: O): RowStream<RowFor<O>>;
  /** Whether the server answers. */
  ping(options?: OperationOptions): Promise<boolean>;
  /** Refuse new work, cancel what runs, and resolve once every connection is released. */
  close(): Promise<void>;
  [Symbol.asyncDispose](): Promise<void>;
}

/** A {@link Pool} whose server has answered a ping. */
export declare function connect(dsn: string, options?: PoolOptions): Promise<Pool>;

/**
 * One connection checked out of a pool, the caller's until it closes it. One
 * operation runs on it at a time: another started meanwhile is a
 * {@link LifecycleError}, as is any while a transaction or stream holds it.
 */
export declare class Connection extends Queryable implements AsyncDisposable {
  private constructor();
  readonly closed: boolean;
  /** Open a transaction, which holds the connection until it commits or rolls back. */
  begin(options?: TransactionOptions): Promise<Transaction>;
  /** Run `use` in a transaction: committed when it resolves, rolled back when it throws. */
  transaction<T>(use: (transaction: Transaction) => Promise<T>, options?: TransactionOptions): Promise<T>;
  /**
   * The rows of `sql`, one per pull. The stream holds the connection until its
   * last row, which frees it, or until it is closed early, which discards it.
   */
  stream<O extends QueryOptions = QueryOptions>(sql: string, params?: readonly Param[], options?: O): RowStream<RowFor<O>>;
  ping(options?: OperationOptions): Promise<boolean>;
  /** Return the connection to its pool, ending a transaction or stream that holds it; resolves once released. */
  close(): Promise<void>;
  [Symbol.asyncDispose](): Promise<void>;
}

/**
 * A transaction, or a savepoint inside one. It borrows its parent
 * exclusively: while a savepoint is open, its transaction refuses statements,
 * commits, rollbacks and other savepoints with a {@link LifecycleError}, and
 * so does a transaction already running a statement.
 */
export declare class Transaction extends Queryable implements AsyncDisposable {
  private constructor();
  readonly closed: boolean;
  /** The connection, or for a savepoint the transaction, this one borrows. */
  readonly parent: Connection | Transaction;
  /** Open a savepoint, which holds this transaction until it finishes. */
  begin(options?: OperationOptions): Promise<Transaction>;
  /** Run `use` in a savepoint: released when it resolves, rolled back to when it throws. */
  transaction<T>(use: (transaction: Transaction) => Promise<T>, options?: OperationOptions): Promise<T>;
  commit(): Promise<void>;
  rollback(): Promise<void>;
  /** Roll back if still open, and resolve once the parent is free. */
  close(): Promise<void>;
  [Symbol.asyncDispose](): Promise<void>;
}

// [spec:pgorm:req:napi.streams]
/**
 * A statement's rows as an async iterator, one row per pull. Its connection
 * is released at the last row, by `close()`, or when a `for await` loop leaves
 * early; closed before its last row, the connection is discarded.
 */
export declare class RowStream<R = Row> implements AsyncIterableIterator<R>, AsyncDisposable {
  private constructor();
  readonly closed: boolean;
  next(): Promise<IteratorResult<R, undefined>>;
  return(): Promise<IteratorResult<R, undefined>>;
  close(): Promise<void>;
  [Symbol.asyncIterator](): AsyncIterableIterator<R>;
  [Symbol.asyncDispose](): Promise<void>;
}
