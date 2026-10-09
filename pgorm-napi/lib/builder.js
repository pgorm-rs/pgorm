// @ts-check

// What every statement and expression builder shares: the native half that
// holds pgorm-query's builder state, and how a builder's arguments reach the
// addon. A builder never changes: each method returns a new one, built from a
// copy of its receiver's state, so a statement or expression can be reused
// and extended in two directions without either affecting the other.
// [spec:pgorm:req:napi.statements]

import { native } from "./operations.js";

/** Marks a construction from a native half the addon made. */
export const TRUSTED = Symbol("pgorm-napi builder state");

/** @type {(builder: Builder) => unknown} */
let handleOf;

/** The base of every builder class; its native half is private. */
export class Builder {
  /** @type {unknown} */
  #handle;

  /** @param {unknown} handle */
  constructor(handle) {
    this.#handle = handle;
  }

  static {
    handleOf = (builder) => builder.#handle;
  }

  /**
   * The SQL and bound values the builder makes, exactly as running it would:
   * a statement as it runs, an expression as the one item of a `SELECT`, a
   * condition as the `WHERE` of `SELECT TRUE`.
   *
   * @returns {import("./index.d.ts").Compiled}
   */
  inspect() {
    const [sql, values] = native.statementInspect(this.#handle);
    return Object.freeze({ sql, values: Object.freeze(values) });
  }
}

/**
 * The native half of `value` when it is a builder, or `value` itself — a
 * value to bind, which the addon reads as a parameter is read.
 *
 * @param {unknown} value
 * @returns {unknown}
 */
export function arg(value) {
  return value instanceof Builder ? handleOf(value) : value;
}

/**
 * Each item of `values` as {@link arg} passes it.
 *
 * @param {Iterable<unknown>} values
 * @returns {unknown[]}
 */
export function args(values) {
  return Array.from(values, arg);
}

/**
 * Refuse a constructor call from outside the module.
 *
 * @param {unknown} trusted
 * @param {string} name
 * @param {string} madeBy
 */
export function trusted(trusted, name, madeBy) {
  if (trusted !== TRUSTED) throw new TypeError(`a ${name} is made by ${madeBy}`);
}

/**
 * What a terminal runs: SQL text with its parameters, or a statement a builder
 * made — which carries its own values, so its options come second — handed
 * to the addon as its native half, built there as `inspect()` builds it.
 *
 * @param {unknown} statement
 * @param {unknown} params
 * @param {unknown} options
 * @returns {[unknown, readonly unknown[], unknown]}
 */
export function statementArgs(statement, params, options) {
  if (statement instanceof Builder) {
    if (Array.isArray(params)) {
      throw new TypeError("a built statement binds its own values: pass its options second");
    }
    return [handleOf(statement), [], params ?? {}];
  }
  return [statement, /** @type {readonly unknown[]} */ (params ?? []), options ?? {}];
}

/**
 * How a builder module makes an expression from its native half without
 * importing the class that defines it, which extends theirs: the expressions
 * module registers the maker as it loads.
 *
 * @type {{ expr: (handle: unknown) => import("./expressions.js").Expr }}
 */
export const made = {
  expr: () => {
    throw new TypeError("pgorm-napi's expressions module has not loaded");
  },
};
