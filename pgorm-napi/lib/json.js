// @ts-check

// SQL/JSON's query functions, constructors, FORMAT JSON and IS JSON over
// pgorm-query's builders. Each function takes only its own behaviours, so a
// choice PostgreSQL refuses for it cannot be written.
// [spec:pgorm:req:napi.sql-json]

import { arg, args, Handle, made, trusted, TRUSTED } from "./builder.js";
import { native } from "./operations.js";

/**
 * @typedef {import("./data-type.js").DataType | string} Returning
 * @typedef {Record<string, unknown>} Passing
 */

/**
 * PASSING variables as `[name, value]` pairs.
 *
 * @param {Passing | undefined} passing
 * @returns {[string, unknown][] | undefined}
 */
export function variables(passing) {
  if (passing === undefined || passing === null) return undefined;
  if (typeof passing !== "object" || Array.isArray(passing)) throw new TypeError("passing is an object of names to values");
  return Object.entries(passing).map(([name, value]) => [name, arg(value)]);
}

/** An operand marked as JSON text, `FORMAT JSON`, accepted only where SQL/JSON reads JSON. */
export class JsonInput extends Handle {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "JsonInput", "formatJson(operand)");
    super(handle);
  }
}

/** `DEFAULT value` for a JSON function's ON EMPTY or ON ERROR, written as an escaped literal. */
export class JsonDefault extends Handle {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "JsonDefault", "jsonDefault(value)");
    super(handle);
  }
}

/** @param {unknown} operand */
export function formatJson(operand) {
  return new JsonInput(native.jsonFormat(arg(operand)), TRUSTED);
}

/** @param {unknown} value */
export function jsonDefault(value) {
  return new JsonDefault(native.jsonDefault(value), TRUSTED);
}

/**
 * @param {unknown} operand
 * @param {{ kind?: "value" | "scalar" | "array" | "object", uniqueKeys?: boolean }} [options]
 */
export function isJson(operand, { kind = "value", uniqueKeys = false } = {}) {
  return made.expr(native.jsonIs(arg(operand), false, kind, uniqueKeys === true));
}

/**
 * @param {unknown} operand
 * @param {{ kind?: "value" | "scalar" | "array" | "object", uniqueKeys?: boolean }} [options]
 */
export function isNotJson(operand, { kind = "value", uniqueKeys = false } = {}) {
  return made.expr(native.jsonIs(arg(operand), true, kind, uniqueKeys === true));
}

/**
 * @param {unknown} context
 * @param {string} path
 * @param {{ passing?: Passing, onError?: "true" | "false" | "unknown" | "error" }} [options]
 */
export function jsonExists(context, path, { passing, onError } = {}) {
  return made.expr(native.jsonExists(arg(context), path, variables(passing), onError));
}

/**
 * @param {unknown} context
 * @param {string} path
 * @param {{ passing?: Passing, returning?: Returning, onEmpty?: "null" | "error" | JsonDefault, onError?: "null" | "error" | JsonDefault }} [options]
 */
export function jsonValue(context, path, { passing, returning, onEmpty, onError } = {}) {
  return made.expr(
    native.jsonValue(arg(context), path, variables(passing), arg(returning), arg(onEmpty), arg(onError)),
  );
}

/**
 * @typedef {"null" | "error" | "emptyArray" | "emptyObject" | JsonDefault} QueryBehavior
 * @typedef {"withWrapper" | "withConditionalWrapper" | "omitQuotes"} Shaping
 */

/**
 * @param {unknown} context
 * @param {string} path
 * @param {{ passing?: Passing, returning?: Returning, shaping?: Shaping, onEmpty?: QueryBehavior, onError?: QueryBehavior }} [options]
 */
export function jsonQuery(context, path, { passing, returning, shaping, onEmpty, onError } = {}) {
  return made.expr(
    native.jsonQuery(arg(context), path, variables(passing), arg(returning), shaping, arg(onEmpty), arg(onError)),
  );
}

/**
 * `JSON_OBJECT`: an object's keys are bound as text; `[key, value]` pairs
 * take any operand as a key, a repeated one included.
 *
 * @param {Record<string, unknown> | readonly (readonly [unknown, unknown])[]} entries
 * @param {{ absentOnNull?: boolean, uniqueKeys?: boolean, returning?: Returning }} [options]
 */
export function jsonObject(entries = {}, { absentOnNull = false, uniqueKeys = false, returning } = {}) {
  const pairs = Array.isArray(entries)
    ? entries.map((pair) => {
      if (!Array.isArray(pair) || pair.length !== 2) throw new TypeError("a JSON object's entry is a [key, value] pair");
      return [arg(pair[0]), arg(pair[1])];
    })
    : Object.entries(entries).map(([key, value]) => [key, arg(value)]);
  return made.expr(native.jsonObject(pairs, absentOnNull === true, uniqueKeys === true, arg(returning)));
}

/**
 * @param {readonly unknown[]} elements
 * @param {{ nullOnNull?: boolean, returning?: Returning }} [options]
 */
export function jsonArray(elements = [], { nullOnNull = false, returning } = {}) {
  return made.expr(native.jsonArray(args(elements), nullOnNull === true, arg(returning)));
}

/**
 * `JSON_ARRAY(SELECT ..)`.
 *
 * @param {import("./select.js").Select} query
 * @param {{ returning?: Returning }} [options]
 */
export function jsonArrayQuery(query, { returning } = {}) {
  return made.expr(native.jsonArrayQuery(arg(query), arg(returning)));
}

/**
 * @param {unknown} key
 * @param {unknown} value
 * @param {{ absentOnNull?: boolean, uniqueKeys?: boolean, returning?: Returning, filter?: unknown }} [options]
 */
export function jsonObjectAgg(key, value, { absentOnNull = false, uniqueKeys = false, returning, filter } = {}) {
  return made.expr(
    native.jsonObjectAgg(arg(key), arg(value), absentOnNull === true, uniqueKeys === true, arg(returning), arg(filter)),
  );
}

/**
 * @param {unknown} value
 * @param {{ orderBy?: readonly import("./expressions.js").OrderBy[], nullOnNull?: boolean, returning?: Returning, filter?: unknown }} [options]
 */
export function jsonArrayAgg(value, { orderBy = [], nullOnNull = false, returning, filter } = {}) {
  return made.expr(native.jsonArrayAgg(arg(value), args(orderBy), nullOnNull === true, arg(returning), arg(filter)));
}

/**
 * `JSON(input)`.
 *
 * @param {unknown} input
 * @param {{ uniqueKeys?: boolean }} [options]
 */
export function jsonParse(input, { uniqueKeys = false } = {}) {
  return made.expr(native.jsonParse(arg(input), uniqueKeys === true));
}

/** @param {unknown} operand */
export function jsonScalar(operand) {
  return made.expr(native.jsonScalar(arg(operand)));
}

/**
 * @param {unknown} input
 * @param {{ returning?: Returning }} [options]
 */
export function jsonSerialize(input, { returning } = {}) {
  return made.expr(native.jsonSerialize(arg(input), arg(returning)));
}
