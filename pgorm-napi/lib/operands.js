// @ts-check

// Comparison, arithmetic and logic: the operators every expression has. An
// operand that is not an expression is a value, bound as a parameter.
// [spec:pgorm:req:napi.expressions]

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
