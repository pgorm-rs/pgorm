// @ts-check

// DDL over pgorm-query's builders: the statements a schema is made of, and the
// tables and columns they create, drop, rename and empty. A DDL statement
// takes no parameters, so each renders with its values as pgorm-query's
// escaped literals and runs as SQL text; `inspect()` gives that SQL and no
// values. Every builder is immutable, as the query builders are.
// [spec:pgorm:req:napi.schema]

import { arg, Builder, Handle, trusted, TRUSTED } from "./builder.js";
import { native } from "./operations.js";

/**
 * @typedef {import("./select.js").Table} Table
 * @typedef {string | Table} Relation
 * @typedef {import("./expressions.js").Expr} Expr
 */

/**
 * Each item of a relation list, or the one relation, as the addon reads it.
 *
 * @param {Relation | readonly Relation[]} relations
 */
export function relations(relations) {
  return Array.isArray(relations) ? relations.map(arg) : arg(relations);
}

/** A complete DDL statement: it inspects as it runs, with no values. */
export class SchemaStatement extends Builder {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "schema statement", "the module's schema functions");
    super(handle);
  }
}

/**
 * @param {unknown} handle
 * @returns {SchemaStatement}
 */
export function statement(handle) {
  return new SchemaStatement(handle, TRUSTED);
}

/** @type {(handle: unknown) => ColumnDef} */
let columnOf;

/**
 * A table's column: its name and type, and the clauses after them in the
 * order they are added. The type may be left out only for `modifyColumn`.
 */
export class ColumnDef extends Handle {
  /**
   * @param {string} name
   * @param {unknown} [type]
   * @param {symbol} [token]
   */
  constructor(name, type, token) {
    if (token === TRUSTED) {
      super(name);
      return;
    }
    super(native.ddlColumnNew(name, arg(type)));
  }

  static {
    columnOf = (handle) => new ColumnDef(/** @type {any} */ (handle), undefined, TRUSTED);
  }

  /** @param {{ name?: string, noInherit?: boolean }} [options] */
  notNull(options) {
    return columnOf(native.ddlColumnNotNull(arg(this), options));
  }

  null() {
    return columnOf(native.ddlColumnNull(arg(this)));
  }

  /** @param {unknown} value */
  default(value) {
    return columnOf(native.ddlColumnDefault(arg(this), arg(value)));
  }

  /**
   * @param {Expr} condition
   * @param {object} [options]
   */
  check(condition, options) {
    return columnOf(native.ddlColumnCheck(arg(this), arg(condition), options));
  }

  /**
   * @param {Expr} expression
   * @param {"stored" | "virtual"} kind
   */
  generated(expression, kind) {
    return columnOf(native.ddlColumnGenerated(arg(this), arg(expression), kind));
  }

  /**
   * @param {"always" | "byDefault"} generation
   * @param {object} [options]
   */
  identity(generation, options) {
    return columnOf(native.ddlColumnIdentity(arg(this), generation, options));
  }

  autoIncrement() {
    return columnOf(native.ddlColumnAutoIncrement(arg(this)));
  }

  /**
   * @param {string} collation
   * @param {{ schema?: string }} [options]
   */
  collate(collation, options = {}) {
    return columnOf(native.ddlColumnCollate(arg(this), { name: collation, schema: options.schema }));
  }
}

/** `CREATE TABLE`: its columns, keys, foreign keys and `CHECK`s. */
export class CreateTable extends SchemaStatement {
  /** @param {ColumnDef} column */
  column(column) {
    return new CreateTable(native.ddlCreateTableColumn(arg(this), arg(column)), TRUSTED);
  }

  ifNotExists() {
    return new CreateTable(native.ddlCreateTableIfNotExists(arg(this)), TRUSTED);
  }

  /**
   * @param {string | readonly string[]} columns
   * @param {object} [options]
   */
  primaryKey(columns, options) {
    return new CreateTable(native.ddlCreateTablePrimaryKey(arg(this), columns, options), TRUSTED);
  }

  /**
   * @param {string | readonly string[]} columns
   * @param {object} [options]
   */
  unique(columns, options) {
    return new CreateTable(native.ddlCreateTableUnique(arg(this), columns, options), TRUSTED);
  }

  /**
   * @param {string | readonly string[]} columns
   * @param {Relation} references
   * @param {string | readonly string[]} refColumns
   * @param {object} [options]
   */
  foreignKey(columns, references, refColumns, options) {
    const handle = native.ddlCreateTableForeignKey(arg(this), columns, arg(references), refColumns, options);
    return new CreateTable(handle, TRUSTED);
  }

  /**
   * @param {Expr} condition
   * @param {object} [options]
   */
  check(condition, options) {
    return new CreateTable(native.ddlCreateTableCheck(arg(this), arg(condition), options), TRUSTED);
  }
}

/** @param {Relation} table */
export function createTable(table) {
  return new CreateTable(native.ddlCreateTable(arg(table)), TRUSTED);
}

/**
 * @param {Relation | readonly Relation[]} tables
 * @param {object} [options]
 */
export function dropTable(tables, options) {
  return statement(native.ddlDropTable(relations(tables), options));
}

/**
 * @param {Relation} table
 * @param {string} name
 */
export function renameTable(table, name) {
  return statement(native.ddlRenameTable(arg(table), name));
}

/**
 * @param {Relation} table
 * @param {string} name
 * @param {string} newName
 */
export function renameColumn(table, name, newName) {
  return statement(native.ddlRenameColumn(arg(table), name, newName));
}

/**
 * @param {Relation} table
 * @param {string} name
 * @param {string} newName
 */
export function renameConstraint(table, name, newName) {
  return statement(native.ddlRenameConstraint(arg(table), name, newName));
}

/** @param {Relation} table */
export function truncateTable(table) {
  return statement(native.ddlTruncateTable(arg(table)));
}

/**
 * @param {Relation} table
 * @param {string} text
 */
export function commentOnTable(table, text) {
  return statement(native.ddlCommentOnTable(arg(table), text));
}

/**
 * @param {Relation} table
 * @param {string} column
 * @param {string} text
 */
export function commentOnColumn(table, column, text) {
  return statement(native.ddlCommentOnColumn(arg(table), column, text));
}
