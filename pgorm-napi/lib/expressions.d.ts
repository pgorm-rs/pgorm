/**
 * Expressions and conditions over pgorm-query's builders.
 *
 * @module
 */

// [spec:pgorm:req:napi.typing]
// [spec:pgorm:req:napi.expressions]

import type { Interval, Param, TypeName, Value } from "./index.d.ts";
import type { Select } from "./select.d.ts";

/** The SQL and the bound values a builder makes, exactly as running it would. */
export interface Compiled {
  readonly sql: string;
  /** Each bound value with its kind, in placeholder order. */
  readonly values: readonly Value[];
}

/**
 * What every statement and expression builder is: immutable, each method
 * returning a new builder from a copy of its receiver's pgorm-query state.
 */
export declare abstract class Builder {
  /**
   * The SQL and values: a statement as it runs, an expression as the one item
   * of a `SELECT`, a condition as the `WHERE` of `SELECT TRUE`.
   */
  inspect(): Compiled;
}

/**
 * A value a built statement binds: a parameter, inferred or declared with
 * {@link Value}, but neither `null` — test with `isNull()`, or bind
 * `Value.null(kind)` — nor an interval, which pgorm's statement values do not
 * hold: bind its text cast, `bind(interval.toString()).cast("interval")`.
 */
export type BindValue = Exclude<Param, null | Interval | Temporal.Duration>;

/** What an operand may be: an expression, or a value to bind. */
export type Operand = Expr | BindValue;

export type Predicate = Expr | Condition;

/** Where NULLs sort. */
export type Nulls = "first" | "last";

/** An SQL expression; every method returns a new one. */
export declare class Expr extends Builder {
  protected constructor();
  eq(other: Operand): Expr;
  ne(other: Operand): Expr;
  lt(other: Operand): Expr;
  lte(other: Operand): Expr;
  gt(other: Operand): Expr;
  gte(other: Operand): Expr;
  add(other: Operand): Expr;
  sub(other: Operand): Expr;
  mul(other: Operand): Expr;
  div(other: Operand): Expr;
  mod(other: Operand): Expr;
  /** `a || b`. */
  concat(other: Operand): Expr;
  isDistinctFrom(other: Operand): Expr;
  isNotDistinctFrom(other: Operand): Expr;
  and(other: Expr): Expr;
  or(other: Expr): Expr;
  not(): Expr;
  isNull(): Expr;
  isNotNull(): Expr;
  /**
   * Membership in a list of operands, or in a subquery's rows. An empty list
   * holds nothing: `isIn([])` is false and `isNotIn([])` true.
   */
  isIn(source: readonly Operand[] | Select): Expr;
  isNotIn(source: readonly Operand[] | Select): Expr;
  between(lower: Operand, upper: Operand, options?: { readonly symmetric?: boolean }): Expr;
  notBetween(lower: Operand, upper: Operand, options?: { readonly symmetric?: boolean }): Expr;
  /** A pattern match; the pattern is bound, and `escape` is one character. */
  like(pattern: string, options?: { readonly escape?: string }): Expr;
  notLike(pattern: string, options?: { readonly escape?: string }): Expr;
  ilike(pattern: string, options?: { readonly escape?: string }): Expr;
  notIlike(pattern: string, options?: { readonly escape?: string }): Expr;
  /** Whether the text begins with `text`, read as text: `%` and `_` match themselves. */
  startsWith(text: Operand): Expr;
  endsWith(text: Operand): Expr;
  containsText(text: Operand): Expr;
  /** `CAST(expr AS type)`, the type named by identifier. */
  cast(type: string | TypeName, options?: { readonly array?: boolean }): Expr;
  /** `(expr COLLATE "collation")`. */
  collate(collation: string, options?: { readonly schema?: string }): Expr;
  /** An array element, `a[index]`; arrays count from 1. */
  at(index: Operand): Expr;
  /** An array slice, `a[lower:upper]`; `null` leaves that end open. */
  slice(lower: Operand | null, upper: Operand | null): Expr;
  /** A projection naming its output column. */
  as(alias: string): Aliased;
  asc(options?: { readonly nulls?: Nulls }): OrderBy;
  desc(options?: { readonly nulls?: Nulls }): OrderBy;
}

/** An expression and the output column name it takes in a projection. */
export declare class Aliased extends Builder {
  private constructor();
}

/** An ORDER BY item, from `expr.asc()` or `expr.desc()`. */
export declare class OrderBy extends Builder {
  private constructor();
}

/** A boolean combination of predicates. */
export declare class Condition extends Builder {
  private constructor();
  /** Their AND; true when there are none. */
  static all(...items: Predicate[]): Condition;
  /** Their OR; false when there are none. */
  static any(...items: Predicate[]): Condition;
  add(item: Predicate): Condition;
  not(): Condition;
}

/** A searched `CASE` with at least one arm. */
export declare class SearchedCase extends Expr {
  private constructor();
  when(condition: Predicate, result: Operand): SearchedCase;
  else(result: Operand): Expr;
}

/** A simple `CASE` with at least one arm. */
export declare class SimpleCase extends Expr {
  private constructor();
  when(value: Operand, result: Operand): SimpleCase;
  else(result: Operand): Expr;
}

/** The operand of a simple `CASE`, which is no expression until it has an arm. */
export declare class CaseOperand extends Builder {
  private constructor();
  when(value: Operand, result: Operand): SimpleCase;
}

/** A column, qualified by a table and its schema when they are given. */
export declare function col(name: string, options?: { readonly table?: string; readonly schema?: string }): Expr;
/** A value bound as a parameter. */
export declare function bind(value: BindValue): Expr;

/** The functions {@link call} reaches, each taking the arguments its pgorm-query constructor does. */
export type FunctionName =
  | "lower"
  | "upper"
  | "abs"
  | "char_length"
  | "count"
  | "count_distinct"
  | "sum"
  | "avg"
  | "min"
  | "max"
  | "round"
  | "coalesce"
  | "random"
  | "gen_random_uuid"
  | "uuidv4"
  | "uuidv7"
  | "uuid_extract_timestamp"
  | "uuid_extract_version";

/**
 * A call to one of the functions pgorm-query has a constructor for; another
 * name or argument count is a {@link ConstructionError}. `uuidv7` takes no
 * argument, or the interval its time is shifted by.
 */
export declare function call(name: FunctionName, ...operands: Operand[]): Expr;
/** `(a, b, ..)`, at least one item. */
export declare function tuple(first: Operand, ...rest: Operand[]): Expr;
/** `EXISTS (select)`. */
export declare function exists(select: Select): Expr;
/** `(select)`, a subquery yielding one value. */
export declare function scalar(select: Select): Expr;
/** A searched `CASE` with its first arm. */
export declare function caseWhen(condition: Predicate, result: Operand): SearchedCase;
/** A simple `CASE` over `operand`; its first `when` makes it an expression. */
export declare function caseOf(operand: Operand): CaseOperand;
