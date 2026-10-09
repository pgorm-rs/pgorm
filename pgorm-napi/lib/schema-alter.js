// @ts-check

// ALTER TABLE over pgorm-query's typestate: `alterTable(table)` names the
// table and has no action, so nothing to inspect or run, and its first action
// gives the statement, which takes more.
// [spec:pgorm:req:napi.schema-alter]

import { arg, Handle, trusted, TRUSTED } from "./builder.js";
import { native } from "./operations.js";
import { SchemaStatement } from "./schema.js";

/**
 * @typedef {import("./schema.js").TableRef} TableRef
 * @typedef {import("./schema.js").ColumnDef} ColumnDef
 * @typedef {import("./expressions.js").Expr} Expr
 * @typedef {string | readonly string[]} Columns
 */

/** @type {(handle: unknown) => AlterTable} */
let alterOf;

/**
 * The actions an `ALTER TABLE` takes, before its first and after it alike.
 *
 * @template {new (...args: any[]) => Handle} B
 * @param {B} Base
 */
function actions(Base) {
  return class extends Base {
    /**
     * @param {ColumnDef} column
     * @param {{ ifNotExists?: boolean }} [options]
     */
    addColumn(column, options) {
      return alterOf(native.ddlAlterAddColumn(arg(this), arg(column), options));
    }

    /** @param {ColumnDef} column */
    modifyColumn(column) {
      return alterOf(native.ddlAlterModifyColumn(arg(this), arg(column)));
    }

    /** @param {string} name */
    dropColumn(name) {
      return alterOf(native.ddlAlterDropColumn(arg(this), name));
    }

    /**
     * @param {Columns} columns
     * @param {object} [options]
     */
    addPrimaryKey(columns, options) {
      return alterOf(native.ddlAlterAddPrimaryKey(arg(this), columns, options));
    }

    /**
     * @param {Columns} columns
     * @param {object} [options]
     */
    addUnique(columns, options) {
      return alterOf(native.ddlAlterAddUnique(arg(this), columns, options));
    }

    /**
     * @param {Columns} columns
     * @param {TableRef} references
     * @param {Columns} refColumns
     * @param {object} [options]
     */
    addForeignKey(columns, references, refColumns, options) {
      return alterOf(native.ddlAlterAddForeignKey(arg(this), columns, arg(references), refColumns, options));
    }

    /**
     * @param {Expr} condition
     * @param {object} [options]
     */
    addCheck(condition, options) {
      return alterOf(native.ddlAlterAddCheck(arg(this), arg(condition), options));
    }

    /**
     * @param {string} column
     * @param {object} [options]
     */
    addNotNull(column, options) {
      return alterOf(native.ddlAlterAddNotNull(arg(this), column, options));
    }

    /**
     * @param {string} name
     * @param {object} [options]
     */
    dropConstraint(name, options) {
      return alterOf(native.ddlAlterDropConstraint(arg(this), name, options));
    }

    /** @param {string} name */
    validateConstraint(name) {
      return alterOf(native.ddlAlterValidateConstraint(arg(this), name));
    }

    /**
     * @param {string} name
     * @param {"inherit" | "noInherit" | "enforced" | "notEnforced"} change
     */
    alterConstraint(name, change) {
      return alterOf(native.ddlAlterAlterConstraint(arg(this), name, change));
    }

    /**
     * @param {string} column
     * @param {Expr} expression
     */
    setExpression(column, expression) {
      return alterOf(native.ddlAlterSetExpression(arg(this), column, arg(expression)));
    }

    /**
     * @param {string} column
     * @param {{ ifExists?: boolean }} [options]
     */
    dropExpression(column, options) {
      return alterOf(native.ddlAlterDropExpression(arg(this), column, options));
    }
  };
}

/** An `ALTER TABLE` before its first action, which PostgreSQL cannot parse. */
export class PendingAlterTable extends actions(Handle) {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "PendingAlterTable", "alterTable(table)");
    super(handle);
  }
}

/** An `ALTER TABLE` with at least one action; every method returns a new one. */
export class AlterTable extends actions(SchemaStatement) {
  static {
    alterOf = (handle) => new AlterTable(handle, TRUSTED);
  }
}

/** @param {TableRef} table */
export function alterTable(table) {
  return new PendingAlterTable(native.ddlAlterTable(arg(table)), TRUSTED);
}
