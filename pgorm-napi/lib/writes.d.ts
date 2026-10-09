/**
 * INSERT, UPDATE and DELETE, ON CONFLICT, and the RETURNING list that reads a
 * written row's old and new versions.
 *
 * @module
 */

// [spec:pgorm:req:napi.typing]
// [spec:pgorm:req:napi.writes]

import type { Aliased, Builder, Expr, Operand, Predicate } from "./expressions.d.ts";
import type { Select, Source, Table, With } from "./select.d.ts";

/** A RETURNING item. */
export type ReturningItem = Expr | Aliased;

/**
 * Renames of a written row's versions, `RETURNING WITH (OLD AS .., NEW AS ..)`,
 * for a statement that has a relation called `old` or `new`; the renamed
 * version answers only to its new name.
 */
export interface ReturningOptions {
  readonly oldAs?: string;
  readonly newAs?: string;
}

/**
 * An `INSERT`; every method returns a new statement. It runs once it has
 * rows: `values`, `select` or `defaultValues`.
 */
export declare class Insert extends Builder {
  private constructor();
  /** The distinct columns each row fills, named once, before any row. */
  columns(first: string, ...rest: string[]): Insert;
  /** One row, an operand for each column. */
  values(...row: Operand[]): Insert;
  /** The rows a query yields, as many columns as the INSERT names. */
  select(query: Select): Insert;
  /** One row of defaults, with no columns named. */
  defaultValues(): Insert;
  /** `OVERRIDING SYSTEM VALUE` or `OVERRIDING USER VALUE`, for identity columns. */
  overriding(which: "systemValue" | "userValue"): Insert;
  onConflict(action: Conflict | ConflictUpdate): Insert;
  /** The RETURNING list; every column when `items` is empty or left out. */
  returning(items?: readonly ReturningItem[], options?: ReturningOptions): Insert;
  with(clause: With): Insert;
}

/**
 * An `UPDATE`; every method returns a new statement. It runs once it has an
 * assignment and a `where` or an explicit `allRows()`.
 */
export declare class Update extends Builder {
  private constructor();
  /** One assignment; each column is assigned once. */
  set(column: string, value: Operand): Update;
  where(predicate: Predicate): Update;
  /** Say the statement means every row it reaches; it removes no predicate. */
  allRows(): Update;
  /** An item the UPDATE reads, `FROM`. */
  from(item: Source): Update;
  returning(items?: readonly ReturningItem[], options?: ReturningOptions): Update;
  with(clause: With): Update;
}

/** A `DELETE`; it runs once it has a `where` or an explicit `allRows()`. */
export declare class Delete extends Builder {
  private constructor();
  where(predicate: Predicate): Delete;
  allRows(): Delete;
  /** An item the DELETE reads, `USING`. */
  using(item: Source): Delete;
  returning(items?: readonly ReturningItem[], options?: ReturningOptions): Delete;
  with(clause: With): Delete;
}

export declare function insert(table: Table): Insert;
export declare function update(table: Table): Update;
export declare function deleteFrom(table: Table): Delete;

/**
 * One version of a written row in a RETURNING list: `old`, the row before the
 * write — every column NULL for an inserted row — or `new`, the row the
 * statement left — every column NULL for a deleted one.
 */
export declare class ReturningRow {
  private constructor();
  static readonly old: ReturningRow;
  static readonly new: ReturningRow;
  col(name: string): Expr;
  star(): Expr;
}

/** A completed conflict action, which `Insert.onConflict` takes. */
export declare class Conflict extends Builder {
  private constructor();
  /** `ON CONFLICT DO NOTHING`, answering any conflict. */
  static doNothing(): Conflict;
  /** An inference target of at least one column name or index expression. */
  static on(first: string | Expr, ...rest: (string | Expr)[]): ConflictTarget;
  /** A constraint named outright, which takes no predicate. */
  static onConstraint(name: string): ConflictTarget;
}

/** A conflict arbiter awaiting its action. */
export declare class ConflictTarget extends Builder {
  private constructor();
  /** The partial index's predicate; an `onConstraint` arbiter refuses one. */
  where(predicate: Predicate): ConflictTarget;
  doNothing(): Conflict;
  /** Set columns from the row that failed to insert, `EXCLUDED`'s. */
  update(first: string, ...rest: string[]): ConflictUpdate;
  set(column: string, value: Operand): ConflictUpdate;
}

/** `DO UPDATE SET ..`, with at least one assignment. */
export declare class ConflictUpdate extends Builder {
  private constructor();
  update(first: string, ...rest: string[]): ConflictUpdate;
  set(column: string, value: Operand): ConflictUpdate;
  /** Update only the conflicting rows that pass `predicate`. */
  where(predicate: Predicate): ConflictUpdate;
}
