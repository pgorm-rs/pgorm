/**
 * SELECT, the tables and other items it reads, and common table expressions.
 *
 * @module
 */

// [spec:pgorm:req:napi.typing]
// [spec:pgorm:req:napi.select]

import type { Aliased, Builder, Expr, OrderBy, Predicate } from "./expressions.d.ts";

/** A table, schema-qualified and aliased when the options say. Its names are identifiers, quoted. */
export declare class Table extends Builder {
  constructor(name: string, options?: { readonly schema?: string; readonly alias?: string });
  as(alias: string): Table;
  /** A column, qualified by the table's alias if it has one, else by its name. */
  col(name: string): Expr;
  /** Every column, `"table".*`. */
  star(): Expr;
}

/** A FROM item that is not a named table, always aliased: a subquery, `JSON_TABLE`. */
export declare class FromItem extends Builder {
  private constructor();
  col(name: string): Expr;
  star(): Expr;
}

/** A source a statement reads. */
export type Source = Table | FromItem;

export type Projection = Expr | Aliased;

/** A `SELECT`; every method returns a new statement. */
export declare class Select extends Builder {
  private constructor();
  /** The projection replaced by at least one item. */
  select(first: Projection, ...rest: Projection[]): Select;
  /** One more FROM item, comma-joined. */
  from(item: Source): Select;
  /**
   * A join on `on`. `lateral` lets a subquery read the columns of the items
   * before it.
   */
  join(
    item: Source,
    on: Predicate,
    options?: { readonly kind?: "inner" | "left" | "right" | "full"; readonly lateral?: boolean },
  ): Select;
  crossJoin(item: Source): Select;
  /** A predicate ANDed to the WHERE clause. */
  where(predicate: Predicate): Select;
  groupBy(...expressions: Expr[]): Select;
  having(predicate: Predicate): Select;
  orderBy(...orderings: OrderBy[]): Select;
  /** LIMIT, or none with `null`: a non-negative integer within PostgreSQL's `bigint`. */
  limit(count: number | bigint | null): Select;
  offset(count: number | bigint | null): Select;
  distinct(): Select;
  union(other: Select): Select;
  unionAll(other: Select): Select;
  intersect(other: Select): Select;
  intersectAll(other: Select): Select;
  except(other: Select): Select;
  exceptAll(other: Select): Select;
  /** Lock the rows read: `FOR UPDATE` and the weaker strengths, `OF` some items, without waiting. */
  lock(
    strength: "update" | "noKeyUpdate" | "share" | "keyShare",
    options?: { readonly of?: readonly Source[]; readonly wait?: "nowait" | "skipLocked" },
  ): Select;
  /** The statement's WITH clause, replacing any it had. */
  with(clause: With): Select;
  /** This query as a FROM item, `(SELECT ..) AS "alias"`. */
  as(alias: string): FromItem;
}

/** `SELECT items`, or `SELECT *` when there are none. */
export declare function select(...items: Projection[]): Select;

/** A common table expression's column names and materialization. */
export interface CteOptions {
  readonly columns?: readonly string[];
  readonly materialized?: boolean;
}

/** What a common table expression's rows come from. */
export type CteBody = Select;

/** A WITH clause: common table expressions a statement reads by name. */
export declare class With extends Builder {
  constructor(name: string, body: CteBody, options?: CteOptions);
  /** One more common table expression, which may read the ones before it. */
  cte(name: string, body: CteBody, options?: CteOptions): With;
  /**
   * `WITH RECURSIVE`, of exactly one common table expression, whose body may
   * read itself, with SEARCH and CYCLE when given.
   */
  static recursive(
    name: string,
    body: Select,
    options?: CteOptions & {
      readonly search?: { readonly order: "breadth" | "depth"; readonly by: Expr; readonly set: string };
      readonly cycle?: { readonly by: Expr; readonly set: string; readonly using: string };
    },
  ): With;
}
