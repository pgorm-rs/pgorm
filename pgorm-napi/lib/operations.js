// @ts-check

// What the connection classes share: the addon they call, the abort tokens an
// `AbortSignal` fires, and how a row's values become an object.
// [spec:pgorm:req:napi.cancellation]

/** @type {any} */
export let native;

/**
 * Hand the module the addon its classes call.
 *
 * @param {any} addon
 */
export function installConnections(addon) {
  native = addon;
}

/** Marks a construction from a handle the addon made. */
export const TRUSTED = Symbol("pgorm-napi native handle");

/**
 * @param {unknown} signal
 * @returns {AbortSignal | undefined}
 */
export function signalOf(signal) {
  if (signal === undefined) return undefined;
  if (!(signal instanceof AbortSignal)) throw new TypeError("options.signal is an AbortSignal");
  return signal;
}

/**
 * Run `start` with a native abort token that `signal` fires, rejecting with
 * the signal's reason once it has: an aborted operation's outcome is unknown,
 * and the addon has discarded the connection it ran on.
 *
 * @template T
 * @param {AbortSignal | undefined} signal
 * @param {(token: unknown) => Promise<T>} start
 * @returns {Promise<T>}
 */
export async function abortable(signal, start) {
  if (signal === undefined) return await start(undefined);
  signal.throwIfAborted();
  const token = native.abortToken();
  const onAbort = () => native.abort(token);
  signal.addEventListener("abort", onAbort, { once: true });
  try {
    return await start(token);
  } catch (error) {
    if (signal.aborted) throw signal.reason;
    throw error;
  } finally {
    signal.removeEventListener("abort", onAbort);
  }
}

/**
 * Each row an object keyed by column name, in column order. Built with
 * `Object.fromEntries`, which defines its keys, so a column named
 * `__proto__` is a property like any other rather than the object's
 * prototype.
 * [spec:pgorm:req:napi.rows]
 *
 * @param {string[]} names
 * @param {unknown[]} values
 */
export function row(names, values) {
  return Object.fromEntries(names.map((name, index) => [name, values[index]]));
}

/**
 * @param {unknown} options
 * @returns {{ tagged: boolean, signal: AbortSignal | undefined }}
 */
export function queryOptions(options) {
  if (typeof options !== "object" || options === null) throw new TypeError("options is an object");
  const { tagged = false, signal } = /** @type {{ tagged?: unknown, signal?: unknown }} */ (options);
  if (typeof tagged !== "boolean") throw new TypeError("options.tagged is a boolean");
  return { tagged, signal: signalOf(signal) };
}

