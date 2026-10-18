// @ts-check

// Set-returning functions read as a FROM item: one call, or several zipped
// side by side under ROWS FROM, the shorter ones padded with NULLs.
// [spec:pgorm:req:napi.select+1]

import { arg, args, TRUSTED } from "./builder.js";
import { native } from "./operations.js";
import { FromItem } from "./select.js";

/**
 * `f(..) AS alias`, or `ROWS FROM (f(..), g(..)) AS alias` over several
 * calls or when `rowsFrom` says; `ordinality` adds a final `bigint` column
 * numbering the rows from 1, and `columns` names the columns in the alias,
 * the ordinal last. Each call is one `call()` made — `generate_series` and
 * `unnest` among them — and the alias is required, as every FROM item that
 * is not a table has one.
 *
 * @param {import("./expressions.js").Expr | readonly import("./expressions.js").Expr[]} calls
 * @param {{ alias: string, ordinality?: boolean, columns?: readonly string[], rowsFrom?: boolean }} options
 */
export function tableFunction(
  calls,
  { alias, ordinality = false, columns, rowsFrom = false } = /** @type {any} */ ({}),
) {
  const list = Array.isArray(calls) ? args(calls) : [arg(calls)];
  const handle = native.tableFunction(list, alias, ordinality === true, columns, rowsFrom === true);
  return new FromItem(handle, TRUSTED);
}
