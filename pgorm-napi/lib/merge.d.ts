/**
 * MERGE, over pgorm-query's typestate.
 *
 * @module
 */

// [spec:pgorm:req:napi.typing]
// [spec:pgorm:req:napi.merge]

import type { Builder, Operand, Predicate } from "./expressions.d.ts";
import type { Source, Table, With } from "./select.d.ts";
import type { ReturningItem, ReturningOptions } from "./writes.d.ts";

/** An arm's options: a condition that narrows the rows it takes. */
export interface ArmOptions {
  readonly condition?: Predicate;
}

/**
 * A MERGE before its first WHEN arm. PostgreSQL refuses a MERGE with none, so
 * it has no `inspect()`, and a terminal refuses it with a
 * {@link ConstructionError}.
 */
export declare class PendingMerge {
  private constructor();
  /** An arm on a target row the join condition matched. */
  whenMatched(action: MergeUpdate | MergeAction, options?: ArmOptions): Merge;
  /** An arm on a source row no target row matched. */
  whenNotMatched(action: MergeInsert | MergeAction, options?: ArmOptions): Merge;
  /** An arm on a target row no source row matched. */
  whenNotMatchedBySource(action: MergeUpdate | MergeAction, options?: ArmOptions): Merge;
}

/**
 * A MERGE with at least one WHEN arm; every method returns a new statement.
 * Within a kind of row a row takes the first conditional arm whose condition
 * holds, in call order, and otherwise the one unconditional arm, which renders
 * last; a later unconditional arm of a kind replaces the earlier one.
 */
export declare class Merge extends Builder {
  private constructor();
  whenMatched(action: MergeUpdate | MergeAction, options?: ArmOptions): Merge;
  whenNotMatched(action: MergeInsert | MergeAction, options?: ArmOptions): Merge;
  whenNotMatchedBySource(action: MergeUpdate | MergeAction, options?: ArmOptions): Merge;
  returning(items?: readonly ReturningItem[], options?: ReturningOptions): Merge;
  /** `merge_action()` first in the RETURNING list: `INSERT`, `UPDATE` or `DELETE` per row. */
  returningAction(): Merge;
  /** A plain WITH clause; a recursive one is refused, as PostgreSQL refuses it. */
  with(clause: With): Merge;
  /** `ONLY` before the target, leaving tables that inherit from it alone. */
  only(): Merge;
}

/** `MERGE INTO target USING source ON on`, pending until its first arm. */
export declare function merge(target: Table, source: Source, on: Predicate): PendingMerge;

/**
 * What an arm does with its row. A target row is updated, deleted or left
 * alone; a source row is inserted or skipped; any other pairing is a
 * {@link ConstructionError}.
 */
export declare class MergeAction {
  protected constructor();
  /** `UPDATE SET column = value`, for a target row. */
  static update(column: string, value: Operand): MergeUpdate;
  /** `INSERT (column) VALUES (value)`, for a source row. */
  static insert(column: string, value: Operand): MergeInsert;
  /** `DELETE`, for a target row. */
  static delete(): MergeAction;
  /** `DO NOTHING`, for any row; a row left alone is not returned. */
  static doNothing(): MergeAction;
  /** `INSERT DEFAULT VALUES`, for a source row. */
  static insertDefaults(): MergeAction;
}

/** A MERGE update, never without an assignment. */
export declare class MergeUpdate extends MergeAction {
  set(column: string, value: Operand): MergeUpdate;
}

/** A MERGE insert, never without a column. */
export declare class MergeInsert extends MergeAction {
  set(column: string, value: Operand): MergeInsert;
  overriding(which: "systemValue" | "userValue"): MergeInsert;
}
