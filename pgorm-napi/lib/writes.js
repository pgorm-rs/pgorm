// @ts-check

// INSERT, UPDATE and DELETE, and the RETURNING list that reads a written
// row's old and new versions.
// [spec:pgorm:req:napi.writes]

import { arg, args, Builder, made, trusted, TRUSTED } from "./builder.js";
import { native } from "./operations.js";

/**
 * @typedef {import("./expressions.js").Expr | import("./expressions.js").Aliased} ReturningItem
 * @typedef {{ oldAs?: string, newAs?: string }} ReturningOptions
 */

/**
 * The write's RETURNING list: every column when `items` is empty, the row's
 * versions renamed when the options say.
 *
 * @param {Builder} write
 * @param {readonly ReturningItem[]} items
 * @param {ReturningOptions} options
 */
function returning(write, items, { oldAs, newAs } = {}) {
  return native.writeReturning(arg(write), args(items), oldAs, newAs);
}

/** An `INSERT`. Every method returns a new statement. */
export class Insert extends Builder {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "Insert", "insert(table)");
    super(handle);
  }

  /**
   * The distinct columns each row fills, named once, before any row.
   *
   * @param {...string} columns
   */
  columns(...columns) {
    return new Insert(native.insertColumns(arg(this), columns), TRUSTED);
  }

  /**
   * One row, an operand for each column.
   *
   * @param {...unknown} row
   */
  values(...row) {
    return new Insert(native.insertValues(arg(this), args(row)), TRUSTED);
  }

  /**
   * The rows a query yields, as many columns as the INSERT names.
   *
   * @param {import("./select.js").Select} query
   */
  select(query) {
    return new Insert(native.insertSelect(arg(this), arg(query)), TRUSTED);
  }

  /** One row of defaults, with no columns named. */
  defaultValues() {
    return new Insert(native.insertDefaults(arg(this)), TRUSTED);
  }

  /** @param {"systemValue" | "userValue"} which */
  overriding(which) {
    return new Insert(native.insertOverriding(arg(this), which), TRUSTED);
  }

  /** @param {import("./conflicts.js").Conflict | import("./conflicts.js").ConflictUpdate} action */
  onConflict(action) {
    return new Insert(native.insertOnConflict(arg(this), arg(action)), TRUSTED);
  }

  /**
   * @param {readonly ReturningItem[]} [items]
   * @param {ReturningOptions} [options]
   */
  returning(items = [], options) {
    return new Insert(returning(this, items, options), TRUSTED);
  }

  /** @param {import("./select.js").With} clause */
  with(clause) {
    return new Insert(native.writeWith(arg(this), arg(clause)), TRUSTED);
  }
}

/** An `UPDATE`. Every method returns a new statement. */
export class Update extends Builder {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "Update", "update(table)");
    super(handle);
  }

  /**
   * One assignment; each column is assigned once.
   *
   * @param {string} column
   * @param {unknown} value
   */
  set(column, value) {
    return new Update(native.updateSet(arg(this), column, arg(value)), TRUSTED);
  }

  /** @param {import("./expressions.js").Expr | import("./conditions.js").Condition} predicate */
  where(predicate) {
    return new Update(native.writeWhere(arg(this), arg(predicate)), TRUSTED);
  }

  /** Say the statement means every row it reaches; it removes no predicate. */
  allRows() {
    return new Update(native.writeAllRows(arg(this)), TRUSTED);
  }

  /** @param {import("./select.js").Table | import("./select.js").FromItem} item */
  from(item) {
    return new Update(native.writeFrom(arg(this), arg(item)), TRUSTED);
  }

  /**
   * @param {readonly ReturningItem[]} [items]
   * @param {ReturningOptions} [options]
   */
  returning(items = [], options) {
    return new Update(returning(this, items, options), TRUSTED);
  }

  /** @param {import("./select.js").With} clause */
  with(clause) {
    return new Update(native.writeWith(arg(this), arg(clause)), TRUSTED);
  }
}

/** A `DELETE`. Every method returns a new statement. */
export class Delete extends Builder {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "Delete", "deleteFrom(table)");
    super(handle);
  }

  /** @param {import("./expressions.js").Expr | import("./conditions.js").Condition} predicate */
  where(predicate) {
    return new Delete(native.writeWhere(arg(this), arg(predicate)), TRUSTED);
  }

  /** Say the statement means every row it reaches; it removes no predicate. */
  allRows() {
    return new Delete(native.writeAllRows(arg(this)), TRUSTED);
  }

  /** @param {import("./select.js").Table | import("./select.js").FromItem} item */
  using(item) {
    return new Delete(native.writeFrom(arg(this), arg(item)), TRUSTED);
  }

  /**
   * @param {readonly ReturningItem[]} [items]
   * @param {ReturningOptions} [options]
   */
  returning(items = [], options) {
    return new Delete(returning(this, items, options), TRUSTED);
  }

  /** @param {import("./select.js").With} clause */
  with(clause) {
    return new Delete(native.writeWith(arg(this), arg(clause)), TRUSTED);
  }
}

/** @param {import("./select.js").Table} table */
export function insert(table) {
  return new Insert(native.insertNew(arg(table)), TRUSTED);
}

/** @param {import("./select.js").Table} table */
export function update(table) {
  return new Update(native.updateNew(arg(table)), TRUSTED);
}

/** @param {import("./select.js").Table} table */
export function deleteFrom(table) {
  return new Delete(native.deleteNew(arg(table)), TRUSTED);
}

/**
 * One version of a written row, read in a RETURNING list: `old`, the row
 * before the write, or `new`, the row the statement left.
 */
export class ReturningRow {
  /** @type {"old" | "new"} */
  #version;

  /**
   * @param {"old" | "new"} version
   * @param {symbol} [token]
   */
  constructor(version, token) {
    trusted(token, "ReturningRow", "ReturningRow.old and ReturningRow.new");
    this.#version = version;
    Object.freeze(this);
  }

  /** @param {string} name */
  col(name) {
    return made.expr(native.returningCol(this.#version, name));
  }

  star() {
    return made.expr(native.returningStar(this.#version));
  }

  static old = new ReturningRow("old", TRUSTED);
  static new = new ReturningRow("new", TRUSTED);
}
