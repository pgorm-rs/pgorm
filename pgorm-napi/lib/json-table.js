// @ts-check

// JSON_TABLE, a FROM item reading rows out of a JSON document. Its paths are
// written as escaped literals, which PostgreSQL requires there; its PASSING
// values are bound.
// [spec:pgorm:req:napi.sql-json]

import { arg, args, Handle, trusted, TRUSTED } from "./builder.js";
import { variables } from "./json.js";
import { native } from "./operations.js";
import { FromItem } from "./select.js";

/**
 * @typedef {import("./data-type.js").DataType | string} ColumnKind
 * @typedef {"null" | "error" | import("./json.js").JsonDefault} ValueBehavior
 * @typedef {"null" | "error" | "emptyArray" | "emptyObject" | import("./json.js").JsonDefault} QueryBehavior
 */

/** One column of a `JSON_TABLE`, each kind taking only its own clauses. */
export class JsonTableColumn extends Handle {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "JsonTableColumn", "JsonTableColumn's functions");
    super(handle);
  }

  /**
   * `name FOR ORDINALITY`: the row's number, from 1.
   *
   * @param {string} name
   */
  static ordinality(name) {
    return new JsonTableColumn(native.jsonTableOrdinality(name), TRUSTED);
  }

  /**
   * The scalar its path finds — `$."name"` unless `path` says — read as JSON_VALUE reads it.
   *
   * @param {string} name
   * @param {ColumnKind} kind
   * @param {{ path?: string, onEmpty?: ValueBehavior, onError?: ValueBehavior }} [options]
   */
  static value(name, kind, { path, onEmpty, onError } = {}) {
    return new JsonTableColumn(native.jsonTableValue(name, arg(kind), path, arg(onEmpty), arg(onError)), TRUSTED);
  }

  /**
   * The JSON its path finds, read as JSON_QUERY reads it.
   *
   * @param {string} name
   * @param {ColumnKind} kind
   * @param {{ path?: string, shaping?: import("./json.js").Shaping, onEmpty?: QueryBehavior, onError?: QueryBehavior }} [options]
   */
  static query(name, kind, { path, shaping, onEmpty, onError } = {}) {
    return new JsonTableColumn(
      native.jsonTableQuery(name, arg(kind), path, shaping, arg(onEmpty), arg(onError)),
      TRUSTED,
    );
  }

  /**
   * Whether its path finds anything; finding nothing is false, so there is no `onEmpty`.
   *
   * @param {string} name
   * @param {ColumnKind} kind
   * @param {{ path?: string, onError?: "true" | "false" | "unknown" | "error" }} [options]
   */
  static exists(name, kind, { path, onError } = {}) {
    return new JsonTableColumn(native.jsonTableExists(name, arg(kind), path, onError), TRUSTED);
  }

  /**
   * `NESTED PATH path COLUMNS (..)`: a row per item its path finds under the
   * parent row's, joined to it as an outer join would be.
   *
   * @param {string} path
   * @param {readonly JsonTableColumn[]} columns
   * @param {{ pathName?: string }} [options]
   */
  static nested(path, columns, { pathName } = {}) {
    return new JsonTableColumn(native.jsonTableNested(path, args(columns), pathName), TRUSTED);
  }
}

/**
 * `JSON_TABLE(context, path COLUMNS (..)) AS alias`, a FROM item: at least
 * one column, and its alias, which every FROM item that is not a table has.
 *
 * @param {unknown} context
 * @param {string} path
 * @param {readonly JsonTableColumn[]} columns
 * @param {{ alias: string, passing?: Record<string, unknown>, pathName?: string, onError?: "error" | "empty" }} options
 */
export function jsonTable(context, path, columns, { alias, passing, pathName, onError } = /** @type {any} */ ({})) {
  const handle = native.jsonTable(arg(context), path, args(columns), alias, variables(passing), pathName, onError);
  return new FromItem(handle, TRUSTED);
}
