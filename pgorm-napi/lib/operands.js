// @ts-check

// Comparison, arithmetic and logic: the operators every expression has. An
// operand that is not an expression is a value, bound as a parameter.
// [spec:pgorm:req:napi.expressions+5]

import { arg, Builder, made } from "./builder.js";
import { native } from "./operations.js";

/**
 * @param {Operand} expr
 * @param {string} operator
 * @param {unknown} other
 */
function binary(expr, operator, other) {
  return made.expr(native.exprBinary(arg(expr), operator, arg(other)));
}

/** What every expression is: the operators. Every method returns a new expression. */
export class Operand extends Builder {
  /** @param {unknown} other */
  eq(other) {
    return binary(this, "eq", other);
  }

  /** @param {unknown} other */
  ne(other) {
    return binary(this, "ne", other);
  }

  /** @param {unknown} other */
  lt(other) {
    return binary(this, "lt", other);
  }

  /** @param {unknown} other */
  lte(other) {
    return binary(this, "lte", other);
  }

  /** @param {unknown} other */
  gt(other) {
    return binary(this, "gt", other);
  }

  /** @param {unknown} other */
  gte(other) {
    return binary(this, "gte", other);
  }

  /** @param {unknown} other */
  add(other) {
    return binary(this, "add", other);
  }

  /** @param {unknown} other */
  sub(other) {
    return binary(this, "sub", other);
  }

  /** @param {unknown} other */
  mul(other) {
    return binary(this, "mul", other);
  }

  /** @param {unknown} other */
  div(other) {
    return binary(this, "div", other);
  }

  /** @param {unknown} other */
  mod(other) {
    return binary(this, "mod", other);
  }

  /** @param {unknown} other */
  concat(other) {
    return binary(this, "concat", other);
  }

  /**
   * `@>`: a range, multirange or array that contains the other.
   * [spec:pgorm:req:napi.ranges+1]
   *
   * @param {unknown} other
   */
  contains(other) {
    return binary(this, "contains", other);
  }

  /**
   * `<@`: contained by the other.
   *
   * @param {unknown} other
   */
  containedBy(other) {
    return binary(this, "containedBy", other);
  }

  /**
   * `&&`: shares a value with the other.
   *
   * @param {unknown} other
   */
  overlaps(other) {
    return binary(this, "overlaps", other);
  }

  /**
   * `<<`: a range or multirange lying wholly left of the other.
   * [spec:pgorm:req:napi.ranges+1]
   *
   * @param {unknown} other
   */
  strictlyLeftOf(other) {
    return binary(this, "strictlyLeftOf", other);
  }

  /**
   * `>>`: a range or multirange lying wholly right of the other.
   *
   * @param {unknown} other
   */
  strictlyRightOf(other) {
    return binary(this, "strictlyRightOf", other);
  }

  /**
   * `&<`: extending no further right than the other.
   *
   * @param {unknown} other
   */
  notExtendRightOf(other) {
    return binary(this, "notExtendRightOf", other);
  }

  /**
   * `&>`: extending no further left than the other.
   *
   * @param {unknown} other
   */
  notExtendLeftOf(other) {
    return binary(this, "notExtendLeftOf", other);
  }

  /**
   * `-|-`: adjacent to the other, sharing no value with nothing between.
   *
   * @param {unknown} other
   */
  adjacentTo(other) {
    return binary(this, "adjacentTo", other);
  }

  /**
   * `~`: whether the text matches a POSIX regular expression, case
   * sensitively; the pattern is bound unless it is an expression.
   *
   * @param {unknown} pattern
   */
  regex(pattern) {
    return binary(this, "regex", pattern);
  }

  /**
   * `!~`: the text does not match the regular expression.
   *
   * @param {unknown} pattern
   */
  notRegex(pattern) {
    return binary(this, "notRegex", pattern);
  }

  /**
   * `~*`: the text matches the regular expression, case ignored.
   *
   * @param {unknown} pattern
   */
  iregex(pattern) {
    return binary(this, "iregex", pattern);
  }

  /**
   * `!~*`: the text does not match the regular expression, case ignored.
   *
   * @param {unknown} pattern
   */
  notIregex(pattern) {
    return binary(this, "notIregex", pattern);
  }

  /**
   * `@?`: whether a `jsonpath` returns any item for this `jsonb`; the path,
   * a string, is bound as text cast to `jsonpath`.
   *
   * @param {string} path
   */
  pathExists(path) {
    return made.expr(native.exprPathExists(arg(this), path));
  }

  /** @param {unknown} other */
  isDistinctFrom(other) {
    return binary(this, "isDistinctFrom", other);
  }

  /** @param {unknown} other */
  isNotDistinctFrom(other) {
    return binary(this, "isNotDistinctFrom", other);
  }

  /** @param {import("./expressions.js").Expr} other */
  and(other) {
    return made.expr(native.exprLogic(arg(this), "and", arg(other)));
  }

  /** @param {import("./expressions.js").Expr} other */
  or(other) {
    return made.expr(native.exprLogic(arg(this), "or", arg(other)));
  }

  not() {
    return made.expr(native.exprUnary(arg(this), "not"));
  }
}
