// @ts-check

// Pools, connections and transactions. Each holds a native handle and
// releases what it holds when closed — `close()`, `await using`, the end of a
// callback — never by waiting for garbage collection.
// [spec:pgorm:req:napi.connections]
// [spec:pgorm:req:napi.results]
// [spec:pgorm:req:napi.transactions]

import { abortable, native, queryOptions, row, signalOf, TRUSTED } from "./operations.js";
import { RowStream } from "./streams.js";

/** The statement runner each class implements, keyed privately. */
const RUN = Symbol("run");

/** Ends a transaction at once, discarding its connection. */
const ABORT = Symbol("abort");

/**
 * What a pool, a connection and a transaction run statements through: bound
 * SQL, its result decoded to rows. Every method that does I/O is an async
 * function, so an argument the addon refuses synchronously rejects its
 * promise rather than throwing at the call site.
 * [spec:pgorm:req:napi.promises]
 */
export class Queryable {
  /**
   * @param {"execute" | "all" | "one" | "optional"} _terminal
   * @param {string} _sql
   * @param {readonly unknown[]} _params
   * @param {boolean} _tagged
   * @param {unknown} _token
   * @returns {Promise<any>}
   */
  [RUN](_terminal, _sql, _params, _tagged, _token) {
    return Promise.reject(new TypeError("Queryable is implemented by Pool, Connection and Transaction"));
  }

  /**
   * @param {"all" | "one" | "optional"} terminal
   * @param {string} sql
   * @param {readonly unknown[]} params
   * @param {unknown} options
   * @returns {Promise<Record<string, unknown>[]>}
   */
  async #rows(terminal, sql, params, options) {
    const { tagged, signal } = queryOptions(options);
    const [names, rows] = await abortable(signal, (token) => this[RUN](terminal, sql, params, tagged, token));
    return rows.map((/** @type {unknown[]} */ values) => row(names, values));
  }

  /**
   * Run a statement and resolve with the number of rows it affected.
   *
   * @param {string} sql
   * @param {readonly unknown[]} [params]
   * @param {{ signal?: AbortSignal }} [options]
   * @returns {Promise<number>}
   */
  async execute(sql, params = [], options = {}) {
    const { signal } = queryOptions(options);
    return await abortable(signal, (token) => this[RUN]("execute", sql, params, false, token));
  }

  /**
   * Every row.
   *
   * @param {string} sql
   * @param {readonly unknown[]} [params]
   * @param {{ tagged?: boolean, signal?: AbortSignal }} [options]
   */
  async query(sql, params = [], options = {}) {
    return await this.#rows("all", sql, params, options);
  }

  /**
   * Exactly one row; any other count is a `DecodeError`.
   *
   * @param {string} sql
   * @param {readonly unknown[]} [params]
   * @param {{ tagged?: boolean, signal?: AbortSignal }} [options]
   */
  async one(sql, params = [], options = {}) {
    return (await this.#rows("one", sql, params, options))[0];
  }

  /**
   * At most one row, or `null`; more is a `DecodeError`.
   *
   * @param {string} sql
   * @param {readonly unknown[]} [params]
   * @param {{ tagged?: boolean, signal?: AbortSignal }} [options]
   */
  async optional(sql, params = [], options = {}) {
    return (await this.#rows("optional", sql, params, options))[0] ?? null;
  }
}

/** @type {(connection: Connection) => unknown} */
let connectionHandle;

/**
 * A pool of connections to one server, opened as they are first needed. TLS
 * verifies the server's certificate and host name unless the connection
 * string says `sslmode=disable` or `tls` is `"disable"`.
 */
export class Pool extends Queryable {
  /** @type {unknown} */
  #native;

  /**
   * @param {string} dsn
   * @param {import("./index.d.ts").PoolOptions} [options]
   */
  constructor(dsn, options = {}) {
    super();
    if (typeof dsn !== "string") throw new TypeError("dsn is a connection string");
    if (typeof options !== "object" || options === null) throw new TypeError("options is an object");
    this.#native = native.poolNew(dsn, options);
  }

  /** @returns {boolean} */
  get closed() {
    return native.poolClosed(this.#native);
  }

  /** @returns {import("./index.d.ts").PoolStatus} */
  status() {
    return native.poolStatus(this.#native);
  }

  /**
   * A connection of the caller's own until it closes it.
   *
   * @param {{ signal?: AbortSignal }} [options]
   * @returns {Promise<Connection>}
   */
  async acquire(options = {}) {
    const { signal } = queryOptions(options);
    const handle = await abortable(signal, (token) => native.poolAcquire(this.#native, token));
    return new Connection(handle, TRUSTED);
  }

  /**
   * Run `use` with a connection, closed when it settles.
   *
   * @template T
   * @param {(connection: Connection) => Promise<T>} use
   * @param {{ signal?: AbortSignal }} [options]
   * @returns {Promise<T>}
   */
  async connection(use, options = {}) {
    const connection = await this.acquire(options);
    try {
      return await use(connection);
    } finally {
      await connection.close();
    }
  }

  /**
   * Run `use` in a transaction on a connection of its own: committed when
   * `use` resolves, rolled back when it throws.
   *
   * @template T
   * @param {(transaction: Transaction) => Promise<T>} use
   * @param {import("./index.d.ts").TransactionOptions} [options]
   * @returns {Promise<T>}
   */
  async transaction(use, options = {}) {
    return await this.connection((connection) => connection.transaction(use, options), {
      signal: options.signal,
    });
  }

  /**
   * The rows of `sql`, pulled one at a time over a connection the stream
   * holds until its last row or until it is closed.
   *
   * @param {string} sql
   * @param {readonly unknown[]} [params]
   * @param {{ tagged?: boolean, signal?: AbortSignal }} [options]
   * @returns {RowStream}
   */
  stream(sql, params = [], options = {}) {
    const pool = this;
    return new RowStream(async (token) => {
      const connection = new Connection(await native.poolAcquire(pool.#native, token), TRUSTED);
      try {
        const handle = await native.connectionStream(connectionHandle(connection), sql, params, token);
        return [handle, () => connection.close()];
      } catch (error) {
        await connection.close();
        throw error;
      }
    }, options, TRUSTED);
  }

  /**
   * Whether the server answers, on a connection of its own.
   *
   * @param {{ signal?: AbortSignal }} [options]
   * @returns {Promise<boolean>}
   */
  async ping(options = {}) {
    return await this.connection((connection) => connection.ping(options), options);
  }

  /**
   * Refuse new work, cancel what runs, and resolve once every connection is
   * released.
   *
   * @returns {Promise<void>}
   */
  async close() {
    await native.poolClose(this.#native);
  }

  [Symbol.asyncDispose]() {
    return this.close();
  }

  /**
   * @override
   * @param {"execute" | "all" | "one" | "optional"} terminal
   * @param {string} sql
   * @param {readonly unknown[]} params
   * @param {boolean} tagged
   * @param {unknown} token
   */
  [RUN](terminal, sql, params, tagged, token) {
    return native.poolRun(this.#native, terminal, sql, params, tagged, token);
  }
}

/**
 * A pool whose server has answered.
 *
 * @param {string} dsn
 * @param {import("./index.d.ts").PoolOptions} [options]
 * @returns {Promise<Pool>}
 */
export async function connect(dsn, options = {}) {
  const pool = new Pool(dsn, options);
  try {
    await pool.ping();
  } catch (error) {
    await pool.close();
    throw error;
  }
  return pool;
}

/**
 * Run `use` in `transaction`: committed when `use` resolves, rolled back when
 * it throws, whose error then stands whatever the rollback does. A commit
 * that fails ends the transaction and discards its connection.
 *
 * @template T
 * @param {Transaction} transaction
 * @param {(transaction: Transaction) => Promise<T>} use
 * @returns {Promise<T>}
 */
async function scoped(transaction, use) {
  let result;
  try {
    result = await use(transaction);
  } catch (error) {
    try {
      await transaction.close();
    } catch {
      // The callback's error stands; a cleanup that failed discarded the
      // connection.
    }
    throw error;
  }
  try {
    await transaction.commit();
  } catch (error) {
    await transaction[ABORT]();
    throw error;
  }
  return result;
}

/** One connection checked out of a pool; one operation at a time. */
export class Connection extends Queryable {
  /** @type {unknown} */
  #native;

  /**
   * @param {unknown} handle
   * @param {symbol} trusted
   */
  constructor(handle, trusted) {
    super();
    if (trusted !== TRUSTED) throw new TypeError("a Connection comes from pool.acquire()");
    this.#native = handle;
  }

  static {
    connectionHandle = (connection) => connection.#native;
  }

  /** @returns {boolean} */
  get closed() {
    return native.connectionClosed(this.#native);
  }

  /**
   * Open a transaction, which holds the connection until it commits or rolls
   * back.
   *
   * @param {import("./index.d.ts").TransactionOptions} [options]
   * @returns {Promise<Transaction>}
   */
  async begin(options = {}) {
    const { mode = "default", isolation = null, signal } = options;
    const handle = await abortable(
      signalOf(signal),
      (token) => native.connectionBegin(this.#native, mode, isolation, token),
    );
    return new Transaction(handle, this, TRUSTED);
  }

  /**
   * Run `use` in a transaction: committed when it resolves, rolled back when
   * it throws.
   *
   * @template T
   * @param {(transaction: Transaction) => Promise<T>} use
   * @param {import("./index.d.ts").TransactionOptions} [options]
   * @returns {Promise<T>}
   */
  async transaction(use, options = {}) {
    return await scoped(await this.begin(options), use);
  }

  /**
   * The rows of `sql`, pulled one at a time; the connection is the stream's
   * until its last row or until it is closed, and a stream closed early
   * discards it.
   *
   * @param {string} sql
   * @param {readonly unknown[]} [params]
   * @param {{ tagged?: boolean, signal?: AbortSignal }} [options]
   * @returns {RowStream}
   */
  stream(sql, params = [], options = {}) {
    const connection = this;
    return new RowStream(
      async (token) => [await native.connectionStream(connection.#native, sql, params, token), undefined],
      options,
      TRUSTED,
    );
  }

  /**
   * @param {{ signal?: AbortSignal }} [options]
   * @returns {Promise<boolean>}
   */
  async ping(options = {}) {
    const { signal } = queryOptions(options);
    return await abortable(signal, (token) => native.connectionPing(this.#native, token));
  }

  /**
   * Return the connection to its pool — or, if a transaction or stream still
   * holds it, end that and discard it — and resolve once it is released.
   *
   * @returns {Promise<void>}
   */
  async close() {
    await native.connectionClose(this.#native);
  }

  [Symbol.asyncDispose]() {
    return this.close();
  }

  /**
   * @override
   * @param {"execute" | "all" | "one" | "optional"} terminal
   * @param {string} sql
   * @param {readonly unknown[]} params
   * @param {boolean} tagged
   * @param {unknown} token
   */
  [RUN](terminal, sql, params, tagged, token) {
    return native.connectionRun(this.#native, terminal, sql, params, tagged, token);
  }
}

/**
 * A transaction, or a savepoint inside one. While a savepoint is open its
 * parent refuses statements, commits, rollbacks and other savepoints.
 */
export class Transaction extends Queryable {
  /** @type {unknown} */
  #native;
  /**
   * The connection or transaction this one borrows, kept reachable so that
   * collecting a parent nobody else holds never ends it under its child.
   *
   * @type {Connection | Transaction}
   */
  #parent;

  /**
   * @param {unknown} handle
   * @param {Connection | Transaction} parent
   * @param {symbol} trusted
   */
  constructor(handle, parent, trusted) {
    super();
    if (trusted !== TRUSTED) throw new TypeError("a Transaction comes from begin() or transaction()");
    this.#native = handle;
    this.#parent = parent;
  }

  /** @returns {Connection | Transaction} */
  get parent() {
    return this.#parent;
  }

  /** @returns {boolean} */
  get closed() {
    return native.transactionClosed(this.#native);
  }

  /**
   * Open a savepoint, which holds this transaction until it finishes.
   *
   * @param {{ signal?: AbortSignal }} [options]
   * @returns {Promise<Transaction>}
   */
  async begin(options = {}) {
    const { signal } = queryOptions(options);
    const handle = await abortable(signal, (token) => native.transactionBegin(this.#native, token));
    return new Transaction(handle, this, TRUSTED);
  }

  /**
   * Run `use` in a savepoint: released when it resolves, rolled back to when
   * it throws.
   *
   * @template T
   * @param {(transaction: Transaction) => Promise<T>} use
   * @param {{ signal?: AbortSignal }} [options]
   * @returns {Promise<T>}
   */
  async transaction(use, options = {}) {
    return await scoped(await this.begin(options), use);
  }

  /** @returns {Promise<void>} */
  async commit() {
    await native.transactionFinish(this.#native, true);
  }

  /** @returns {Promise<void>} */
  async rollback() {
    await native.transactionFinish(this.#native, false);
  }

  /**
   * Roll back if still open, and resolve once the parent is free; a rollback
   * that fails ends the transaction and discards its connection.
   *
   * @returns {Promise<void>}
   */
  async close() {
    if (!this.closed) {
      try {
        await this.rollback();
      } catch (error) {
        await this[ABORT]();
        throw error;
      }
    }
    await native.transactionWaitClosed(this.#native);
  }

  [Symbol.asyncDispose]() {
    return this.close();
  }

  /** @returns {Promise<void>} */
  async [ABORT]() {
    await native.transactionAbort(this.#native);
  }

  /**
   * @override
   * @param {"execute" | "all" | "one" | "optional"} terminal
   * @param {string} sql
   * @param {readonly unknown[]} params
   * @param {boolean} tagged
   * @param {unknown} token
   */
  [RUN](terminal, sql, params, tagged, token) {
    return native.transactionRun(this.#native, terminal, sql, params, tagged, token);
  }
}
