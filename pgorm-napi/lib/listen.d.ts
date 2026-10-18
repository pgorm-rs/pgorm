/**
 * LISTEN and NOTIFY: a dedicated listening connection, opened by
 * `pool.listener()`, whose notifications are an async iterator, and `notify`
 * on a pool, a connection or a transaction.
 *
 * @module
 */

// [spec:pgorm:req:napi.typing]
// [spec:pgorm:req:napi.listen]

import type { OperationOptions } from "./index.d.ts";

/** A notification: the channel it was sent on, its payload, and the process id of the session that sent it. */
export interface Notification {
  readonly channel: string;
  readonly payload: string;
  readonly processId: number;
}

/** How `pool.listener()` opens a listener. */
export interface ListenerOptions extends OperationOptions {
  /**
   * The most notifications held unread, a positive whole number; 1024 by
   * default. One arriving while this many wait closes the listener, whose
   * next pull past them rejects with a {@link ConnectionError}.
   */
  readonly capacity?: number;
}

/**
 * A connection of its own, opened with its pool's configuration and closed
 * with the pool, that receives the notifications sent on the channels it
 * listens on. It is an async iterator of them, one per `next()`, in the order
 * the server sent them: by commit order across transactions, and a payload a
 * transaction repeats on one channel once. Nothing reconnects it. Its
 * iteration ends once it closes — by `close()`, a `for await` loop leaving
 * early, or its pool closing — or after the failure it ends with, which the
 * pull that meets it rejects with: a {@link DatabaseError} or
 * {@link ConnectionError} for a lost connection, or a {@link ConnectionError}
 * for a notification arriving while `capacity` wait unread. One pull runs at a
 * time; a second meanwhile is a {@link LifecycleError}. `listen` and
 * `unlisten` may run while a pull waits.
 */
export declare class Listener implements AsyncIterableIterator<Notification>, AsyncDisposable {
  private constructor();
  readonly closed: boolean;
  /**
   * Receive the notifications sent on `channel`. The name is exact — its case
   * and punctuation are kept — and one longer than 63 bytes is a
   * {@link ConstructionError}, sent nothing, since PostgreSQL would listen on
   * it truncated. Aborted, the listener closes, since the server may have
   * listened.
   */
  listen(channel: string, options?: OperationOptions): Promise<void>;
  /** Stop receiving the notifications sent on `channel`; aborted, the listener closes. */
  unlisten(channel: string, options?: OperationOptions): Promise<void>;
  /** Stop receiving notifications on every channel; aborted, the listener closes. */
  unlistenAll(options?: OperationOptions): Promise<void>;
  /**
   * The next notification. An aborted pull leaves the listener open: the
   * notification it waited for is the next pull's.
   */
  next(options?: OperationOptions): Promise<IteratorResult<Notification, undefined>>;
  /** Close the listener, as a `for await` loop leaving early does. */
  return(): Promise<IteratorResult<Notification, undefined>>;
  /** Close the listener's connection, ending a pull in flight; resolves once it is released. */
  close(): Promise<void>;
  [Symbol.asyncIterator](): AsyncIterableIterator<Notification>;
  [Symbol.asyncDispose](): Promise<void>;
}
