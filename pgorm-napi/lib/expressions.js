// @ts-check

// Expressions over pgorm-query's builders. An operand that is not an
// expression is a value, bound as a parameter: nothing a caller passes becomes
// SQL text, and an identifier is quoted wherever it appears.
// [spec:pgorm:req:napi.expressions]

import { arg, args, Builder, made, trusted, TRUSTED } from "./builder.js";
import { native } from "./operations.js";
import { Predicate } from "./predicates.js";

/**
 * @param {Expr} expr
 * @param {"asc" | "desc"} direction
 * @param {{ nulls?: "first" | "last" }} options
 */
function ordering(expr, direction, { nulls } = {}) {
  return new OrderBy(native.exprOrder(arg(expr), direction, nulls), TRUSTED);
}

/** An SQL expression. Every method returns a new expression. */
export class Expr extends Predicate {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "SQL expression", "col(), bind(), call() and the other expression functions");
    super(handle);
  }

  /**
   * @param {string | import("./values.js").TypeName} type
   * @param {{ array?: boolean }} [options]
   */
  cast(type, { array = false } = {}) {
    return new Expr(native.exprCast(arg(this), type, array === true), TRUSTED);
  }

  /**
   * @param {string} collation
   * @param {{ schema?: string }} [options]
   */
  collate(collation, { schema } = {}) {
    return new Expr(native.exprCollate(arg(this), collation, schema), TRUSTED);
  }

  /** @param {unknown} index */
  at(index) {
    return new Expr(native.exprSubscript(arg(this), arg(index)), TRUSTED);
  }

  /**
   * @param {unknown} lower
   * @param {unknown} upper
   */
  slice(lower, upper) {
    return new Expr(native.exprSubscript(arg(this), arg(lower ?? null), arg(upper ?? null), true), TRUSTED);
  }

  /** @param {string} alias */
  as(alias) {
    return new Aliased(native.exprAlias(arg(this), alias), TRUSTED);
  }

  /** @param {{ nulls?: "first" | "last" }} [options] */
  asc(options) {
    return ordering(this, "asc", options);
  }

  /** @param {{ nulls?: "first" | "last" }} [options] */
  desc(options) {
    return ordering(this, "desc", options);
  }
}

made.expr = (handle) => new Expr(handle, TRUSTED);

/** An expression and the name of the output column it makes. */
export class Aliased extends Builder {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "aliased projection", "expr.as(name)");
    super(handle);
  }
}

/** An ORDER BY item. */
export class OrderBy extends Builder {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "ordering", "expr.asc() or expr.desc()");
    super(handle);
  }
}

/**
 * A column, qualified by a table and its schema when they are given.
 *
 * @param {string} name
 * @param {{ table?: string, schema?: string }} [options]
 */
export function col(name, { table, schema } = {}) {
  return new Expr(native.exprCol(name, table, schema), TRUSTED);
}

/**
 * A value bound as a parameter, inferred as a parameter is or declared with
 * `Value`.
 *
 * @param {unknown} value
 */
export function bind(value) {
  return new Expr(native.exprBind(value), TRUSTED);
}

/**
 * A call to one of the functions pgorm-query constructs.
 *
 * @param {string} name
 * @param {...unknown} operands
 */
export function call(name, ...operands) {
  return new Expr(native.exprCall(name, args(operands)), TRUSTED);
}

/** @param {...unknown} items */
export function tuple(...items) {
  return new Expr(native.exprTuple(args(items)), TRUSTED);
}

/**
 * `EXISTS (select)`.
 *
 * @param {import("./select.js").Select} select
 */
export function exists(select) {
  return new Expr(native.exprSubquery("exists", arg(select)), TRUSTED);
}

/**
 * `(select)`, a subquery yielding one value.
 *
 * @param {import("./select.js").Select} select
 */
export function scalar(select) {
  return new Expr(native.exprSubquery("scalar", arg(select)), TRUSTED);
}
