// @ts-check

// Row streams over a pool's or a connection's connection.
// [spec:pgorm:req:napi.streams]

import { LifecycleError } from "./errors.js";
import { abortable, native, queryOptions, row, TRUSTED } from "./operations.js";

/**
 * A statement's rows as an async iterator, one row per pull, so the server is
 * held back while nothing asks. Its connection is released at the last row,
 * by `close()`, or when a `for await` loop leaves early.
 *
 * @implements {AsyncIterableIterator<Record<string, unknown>>}
 */
export class RowStream {
  /** @type {(token: unknown) => Promise<[unknown, (() => Promise<void>) | undefined]>} */
  #open;
  /** @type {unknown} */
  #native = null;
  /** @type {(() => Promise<void>) | undefined} */
  #release;
  /** @type {string[]} */
  #names = [];
  #tagged;
  /** @type {AbortSignal | undefined} */
  #signal;
  #done = false;
  #pulling = false;

  /**
   * @param {(token: unknown) => Promise<[unknown, (() => Promise<void>) | undefined]>} open
   * @param {unknown} options
   * @param {symbol} trusted
   */
  constructor(open, options, trusted) {
    if (trusted !== TRUSTED) throw new TypeError("a RowStream comes from stream()");
    const { tagged, signal } = queryOptions(options);
    this.#open = open;
    this.#tagged = tagged;
    this.#signal = signal;
  }

  /** @returns {boolean} */
  get closed() {
    return this.#done;
  }

  /** @returns {Promise<IteratorResult<Record<string, unknown>, undefined>>} */
  async next() {
    if (this.#done) return { done: true, value: undefined };
    if (this.#pulling) throw new LifecycleError("the stream is busy: one row is pulled at a time");
    this.#pulling = true;
    try {
      return await abortable(this.#signal, async (token) => {
        if (this.#native === null) [this.#native, this.#release] = await this.#open(token);
        const item = await native.streamNext(this.#native, this.#tagged, token);
        if (item === null) {
          await this.#finish();
          return { done: true, value: undefined };
        }
        const [values, names] = item;
        if (names) this.#names = names;
        return { done: false, value: row(this.#names, values) };
      });
    } catch (error) {
      await this.#finish();
      throw error;
    } finally {
      this.#pulling = false;
    }
  }

  /**
   * End the stream, as a `for await` loop does when it leaves early.
   *
   * @returns {Promise<IteratorResult<Record<string, unknown>, undefined>>}
   */
  async return() {
    await this.#finish();
    return { done: true, value: undefined };
  }

  /**
   * End the stream and release its connection: discarded if rows remained.
   *
   * @returns {Promise<void>}
   */
  async close() {
    await this.#finish();
  }

  async #finish() {
    if (this.#done) return;
    this.#done = true;
    try {
      if (this.#native !== null) await native.streamClose(this.#native);
    } finally {
      await this.#release?.();
    }
  }

  [Symbol.asyncIterator]() {
    return this;
  }

  [Symbol.asyncDispose]() {
    return this.close();
  }
}
