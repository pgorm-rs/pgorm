// @ts-check

// MERGE over pgorm-query's typestate: `merge()` gives a `PendingMerge`, which
// has no WHEN arm and so nothing to inspect or run, and its first arm gives
// the `Merge` statement. Each arm's action is checked against the kind of row
// the arm takes.
// [spec:pgorm:req:napi.merge]

import { arg, args, Builder, Handle, trusted, TRUSTED } from "./builder.js";
import { native } from "./operations.js";

/**
 * @typedef {import("./expressions.js").Expr | import("./conditions.js").Condition} Predicate
 * @typedef {{ condition?: Predicate }} ArmOptions
 */

/**
 * @param {Handle} merge
 * @param {"matched" | "notMatched" | "notMatchedBySource"} kind
 * @param {MergeAction} action
 * @param {ArmOptions} options
 */
function arm(merge, kind, action, { condition } = {}) {
  return new Merge(native.mergeWhen(arg(merge), kind, arg(action), arg(condition)), TRUSTED);
}

/** A MERGE before its first WHEN arm; PostgreSQL refuses one with none. */
export class PendingMerge extends Handle {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "PendingMerge", "merge(target, source, on)");
    super(handle);
  }

  /**
   * An arm on a target row the join condition matched: `MergeAction.update`,
   * `delete()` or `doNothing()`.
   *
   * @param {MergeAction} action
   * @param {ArmOptions} [options]
   */
  whenMatched(action, options) {
    return arm(this, "matched", action, options);
  }

  /**
   * An arm on a source row no target row matched: `MergeAction.insert`,
   * `insertDefaults()` or `doNothing()`.
   *
   * @param {MergeAction} action
   * @param {ArmOptions} [options]
   */
  whenNotMatched(action, options) {
    return arm(this, "notMatched", action, options);
  }

  /**
   * An arm on a target row no source row matched, as `whenMatched` takes one.
   *
   * @param {MergeAction} action
   * @param {ArmOptions} [options]
   */
  whenNotMatchedBySource(action, options) {
    return arm(this, "notMatchedBySource", action, options);
  }
}

/** A MERGE with at least one WHEN arm. Every method returns a new statement. */
export class Merge extends Builder {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "Merge", "a PendingMerge's first arm");
    super(handle);
  }

  /**
   * @param {MergeAction} action
   * @param {ArmOptions} [options]
   */
  whenMatched(action, options) {
    return arm(this, "matched", action, options);
  }

  /**
   * @param {MergeAction} action
   * @param {ArmOptions} [options]
   */
  whenNotMatched(action, options) {
    return arm(this, "notMatched", action, options);
  }

  /**
   * @param {MergeAction} action
   * @param {ArmOptions} [options]
   */
  whenNotMatchedBySource(action, options) {
    return arm(this, "notMatchedBySource", action, options);
  }

  /**
   * @param {readonly (import("./expressions.js").Expr | import("./expressions.js").Aliased)[]} [items]
   * @param {{ oldAs?: string, newAs?: string }} [options]
   */
  returning(items = [], { oldAs, newAs } = {}) {
    return new Merge(native.mergeReturning(arg(this), args(items), oldAs, newAs), TRUSTED);
  }

  /** `merge_action()` first in the RETURNING list: `INSERT`, `UPDATE` or `DELETE` per row. */
  returningAction() {
    return new Merge(native.mergeReturningAction(arg(this)), TRUSTED);
  }

  /** @param {import("./select.js").With} clause */
  with(clause) {
    return new Merge(native.mergeWith(arg(this), arg(clause)), TRUSTED);
  }

  /** `ONLY` before the target, leaving tables that inherit from it alone. */
  only() {
    return new Merge(native.mergeOnly(arg(this)), TRUSTED);
  }
}

/**
 * `MERGE INTO target USING source ON on`, pending until its first arm.
 *
 * @param {import("./select.js").Table} target
 * @param {import("./select.js").Table | import("./select.js").FromItem} source
 * @param {Predicate} on
 */
export function merge(target, source, on) {
  return new PendingMerge(native.mergeNew(arg(target), arg(source), arg(on)), TRUSTED);
}

/** What an arm does with its row. */
export class MergeAction extends Handle {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "MergeAction", "MergeAction's functions");
    super(handle);
  }

  /**
   * `UPDATE SET column = value`, for a target row; `set` adds assignments.
   *
   * @param {string} column
   * @param {unknown} value
   */
  static update(column, value) {
    return new MergeUpdate(native.mergeActionUpdate(column, arg(value)), TRUSTED);
  }

  /**
   * `INSERT (column) VALUES (value)`, for a source row; `set` adds columns.
   *
   * @param {string} column
   * @param {unknown} value
   */
  static insert(column, value) {
    return new MergeInsert(native.mergeActionInsert(column, arg(value)), TRUSTED);
  }

  /** `DELETE`, for a target row. */
  static delete() {
    return new MergeAction(native.mergeActionKeyword("delete"), TRUSTED);
  }

  /** `DO NOTHING`, for any row; a row left alone is not returned. */
  static doNothing() {
    return new MergeAction(native.mergeActionKeyword("doNothing"), TRUSTED);
  }

  /** `INSERT DEFAULT VALUES`, for a source row. */
  static insertDefaults() {
    return new MergeAction(native.mergeActionKeyword("insertDefaults"), TRUSTED);
  }
}

/** A MERGE update, never without an assignment. */
export class MergeUpdate extends MergeAction {
  /**
   * @param {string} column
   * @param {unknown} value
   */
  set(column, value) {
    return new MergeUpdate(native.mergeActionSet(arg(this), column, arg(value)), TRUSTED);
  }
}

/** A MERGE insert, never without a column. */
export class MergeInsert extends MergeAction {
  /**
   * @param {string} column
   * @param {unknown} value
   */
  set(column, value) {
    return new MergeInsert(native.mergeActionSet(arg(this), column, arg(value)), TRUSTED);
  }

  /** @param {"systemValue" | "userValue"} which */
  overriding(which) {
    return new MergeInsert(native.mergeActionOverriding(arg(this), which), TRUSTED);
  }
}
