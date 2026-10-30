// @ts-check

// Grouping elements: the items of a GROUP BY list beyond a bare expression.
// Each stands for a list of grouping sets, and a GROUP BY list groups by
// every combination of its items' sets, so two items can reach one set twice
// — which `groupByDistinct()` groups once.
// [spec:pgorm:req:bindings.select]

import { args, Handle, trusted, TRUSTED } from "./builder.js";
import { native } from "./operations.js";

/** An item of a GROUP BY list that is not a bare expression. */
export class GroupingElement extends Handle {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "GroupingElement", "groupingSet(), rollup(), cube() and groupingSets()");
    super(handle);
  }
}

/**
 * One grouping set, `(a, b)`; with no expressions the empty set `()`, which
 * groups the whole input into one row.
 *
 * @param {...import("./expressions.js").Expr} expressions
 */
export function groupingSet(...expressions) {
  return new GroupingElement(native.groupingNew("set", args(expressions)), TRUSTED);
}

/**
 * `ROLLUP (a, b)`: the sets `(a, b)`, `(a)` and `()`, every leading prefix —
 * the subtotals of a hierarchy. A `tuple` item is one unit of it.
 *
 * @param {...import("./expressions.js").Expr} expressions
 */
export function rollup(...expressions) {
  return new GroupingElement(native.groupingNew("rollup", args(expressions)), TRUSTED);
}

/**
 * `CUBE (a, b)`: every subset of the list, `()` included.
 *
 * @param {...import("./expressions.js").Expr} expressions
 */
export function cube(...expressions) {
  return new GroupingElement(native.groupingNew("cube", args(expressions)), TRUSTED);
}

/**
 * `GROUPING SETS (..)`: exactly the sets its items stand for, an expression
 * being the set of itself. PostgreSQL refuses an empty list, so there is at
 * least one item.
 *
 * @param {...(import("./expressions.js").Expr | GroupingElement)} items
 */
export function groupingSets(...items) {
  return new GroupingElement(native.groupingSets(args(items)), TRUSTED);
}
