// @ts-check

// ON CONFLICT: an arbiter, then an action. A target is never empty, an update
// never assigns nothing, and only a completed action reaches an INSERT.
// [spec:pgorm:req:napi.writes]

import { arg, args, Builder, trusted, TRUSTED } from "./builder.js";
import { native } from "./operations.js";

/** A completed conflict action, which `Insert.onConflict` takes. */
export class Conflict extends Builder {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "Conflict", "Conflict.doNothing(), Conflict.on(..) or Conflict.onConstraint(..)");
    super(handle);
  }

  /** `ON CONFLICT DO NOTHING`, answering any conflict. */
  static doNothing() {
    return new Conflict(native.conflictDoNothing(), TRUSTED);
  }

  /**
   * An inference target: at least one column name or index expression.
   *
   * @param {...(string | import("./expressions.js").Expr)} targets
   */
  static on(...targets) {
    return new ConflictTarget(native.conflictOn(args(targets)), TRUSTED);
  }

  /**
   * A constraint named outright, `ON CONSTRAINT "name"`, which takes no predicate.
   *
   * @param {string} name
   */
  static onConstraint(name) {
    return new ConflictTarget(native.conflictOnConstraint(name), TRUSTED);
  }
}

/** A conflict arbiter awaiting its action. */
export class ConflictTarget extends Builder {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "ConflictTarget", "Conflict.on(..) or Conflict.onConstraint(..)");
    super(handle);
  }

  /**
   * The partial index's predicate; an `ON CONSTRAINT` arbiter takes none.
   *
   * @param {import("./expressions.js").Expr | import("./conditions.js").Condition} predicate
   */
  where(predicate) {
    return new ConflictTarget(native.conflictWhere(arg(this), arg(predicate)), TRUSTED);
  }

  doNothing() {
    return new Conflict(native.conflictAction(arg(this)), TRUSTED);
  }

  /**
   * Set columns from the row that failed to insert, `EXCLUDED`'s; at least one.
   *
   * @param {...string} columns
   */
  update(...columns) {
    return new ConflictUpdate(native.conflictUpdate(arg(this), columns), TRUSTED);
  }

  /**
   * @param {string} column
   * @param {unknown} value
   */
  set(column, value) {
    return new ConflictUpdate(native.conflictSet(arg(this), column, arg(value)), TRUSTED);
  }
}

/** `DO UPDATE SET ..`, with at least one assignment. */
export class ConflictUpdate extends Builder {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "ConflictUpdate", "a ConflictTarget's update(..) or set(..)");
    super(handle);
  }

  /** @param {...string} columns */
  update(...columns) {
    return new ConflictUpdate(native.conflictUpdate(arg(this), columns), TRUSTED);
  }

  /**
   * @param {string} column
   * @param {unknown} value
   */
  set(column, value) {
    return new ConflictUpdate(native.conflictSet(arg(this), column, arg(value)), TRUSTED);
  }

  /**
   * Update only the conflicting rows that pass `predicate`.
   *
   * @param {import("./expressions.js").Expr | import("./conditions.js").Condition} predicate
   */
  where(predicate) {
    return new ConflictUpdate(native.conflictWhere(arg(this), arg(predicate)), TRUSTED);
  }
}
