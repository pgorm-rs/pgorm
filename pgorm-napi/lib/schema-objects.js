// @ts-check

// CREATE INDEX and DROP INDEX; sequences, created, altered over pgorm-query's
// typestate, dropped and renamed; and extensions.
// [spec:pgorm:req:napi.schema-indexes]
// [spec:pgorm:req:napi.schema-sequences]

import { arg, Handle, trusted, TRUSTED } from "./builder.js";
import { native } from "./operations.js";
import { relations, SchemaStatement, statement } from "./schema.js";

/**
 * @typedef {import("./schema.js").Relation} Relation
 * @typedef {import("./expressions.js").Expr} Expr
 */

/**
 * An index entry as the addon reads it: `{ on, .. }`'s expression passed as
 * its native half.
 *
 * @param {unknown} entry
 */
function entry(entry) {
  if (entry === null || typeof entry !== "object" || entry instanceof Handle) return arg(entry);
  return { ...entry, on: arg(/** @type {{ on?: unknown }} */ (entry).on) };
}

/** `CREATE INDEX`; every method returns a new statement. */
export class CreateIndex extends SchemaStatement {
  /** @param {unknown} column */
  column(column) {
    return new CreateIndex(native.ddlIndexColumn(arg(this), entry(column)), TRUSTED);
  }

  unique() {
    return new CreateIndex(native.ddlIndexUnique(arg(this)), TRUSTED);
  }

  nullsNotDistinct() {
    return new CreateIndex(native.ddlIndexNullsNotDistinct(arg(this)), TRUSTED);
  }

  ifNotExists() {
    return new CreateIndex(native.ddlIndexIfNotExists(arg(this)), TRUSTED);
  }

  /** @param {string} method */
  using(method) {
    return new CreateIndex(native.ddlIndexUsing(arg(this), method), TRUSTED);
  }

  /** @param {readonly string[]} columns */
  include(columns) {
    return new CreateIndex(native.ddlIndexInclude(arg(this), columns), TRUSTED);
  }

  /** @param {Expr | import("./conditions.js").Condition} predicate */
  where(predicate) {
    return new CreateIndex(native.ddlIndexWhere(arg(this), arg(predicate)), TRUSTED);
  }
}

/**
 * @param {Relation} table
 * @param {unknown} column
 * @param {{ name?: string }} [options]
 */
export function createIndex(table, column, options) {
  return new CreateIndex(native.ddlCreateIndex(arg(table), entry(column), options), TRUSTED);
}

/**
 * @param {Relation} table
 * @param {string} name
 * @param {{ ifExists?: boolean }} [options]
 */
export function dropIndex(table, name, options) {
  return statement(native.ddlDropIndex(arg(table), name, options));
}

/** @type {(handle: unknown) => CreateSequence} */
let createOf;

/** @type {(handle: unknown) => AlterSequence} */
let alterOf;

/**
 * The statement a clause gives: a create stays one, and an alter, pending or
 * not, becomes the statement.
 *
 * @param {Handle} receiver
 * @param {unknown} handle
 * @returns {SchemaStatement}
 */
function next(receiver, handle) {
  return receiver instanceof CreateSequence ? createOf(handle) : alterOf(handle);
}

/**
 * The clauses a sequence statement takes, creating it or altering it.
 *
 * @template {new (...args: any[]) => Handle} B
 * @param {B} Base
 */
function clauses(Base) {
  return class extends Base {
    /** @param {"smallint" | "integer" | "bigint"} type */
    asType(type) {
      return next(this, native.ddlSequenceAsType(arg(this), type));
    }

    /** @param {object} options */
    options(options) {
      return next(this, native.ddlSequenceOptions(arg(this), options));
    }

    /**
     * `OWNED BY` a table's column, or `OWNED BY NONE` with `null`.
     *
     * @param {Relation | null} table
     * @param {string} [column]
     */
    ownedBy(table, column) {
      return next(this, native.ddlSequenceOwnedBy(arg(this), table === null ? null : arg(table), column));
    }
  };
}

/** `CREATE SEQUENCE`; every method returns a new statement. */
export class CreateSequence extends clauses(SchemaStatement) {
  static {
    createOf = (handle) => new CreateSequence(handle, TRUSTED);
  }

  ifNotExists() {
    return createOf(native.ddlSequenceIfNotExists(arg(this)));
  }
}

/** An `ALTER SEQUENCE` before its first clause, which PostgreSQL cannot parse. */
export class PendingAlterSequence extends clauses(Handle) {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "PendingAlterSequence", "alterSequence(name)");
    super(handle);
  }

  /** @param {number | bigint} [value] */
  restart(value) {
    return alterOf(native.ddlSequenceRestart(arg(this), value));
  }
}

/** An `ALTER SEQUENCE` with at least one clause; every method returns a new one. */
export class AlterSequence extends clauses(SchemaStatement) {
  static {
    alterOf = (handle) => new AlterSequence(handle, TRUSTED);
  }

  /** @param {number | bigint} [value] */
  restart(value) {
    return alterOf(native.ddlSequenceRestart(arg(this), value));
  }

  ifExists() {
    return alterOf(native.ddlSequenceIfExists(arg(this)));
  }
}

/** @param {Relation} name */
export function createSequence(name) {
  return createOf(native.ddlCreateSequence(arg(name)));
}

/** @param {Relation} name */
export function alterSequence(name) {
  return new PendingAlterSequence(native.ddlAlterSequence(arg(name)), TRUSTED);
}

/**
 * @param {Relation | readonly Relation[]} names
 * @param {object} [options]
 */
export function dropSequence(names, options) {
  return statement(native.ddlDropSequence(relations(names), options));
}

/**
 * @param {Relation} name
 * @param {string} newName
 */
export function renameSequence(name, newName) {
  return statement(native.ddlRenameSequence(arg(name), newName));
}

/**
 * @param {string} name
 * @param {object} [options]
 */
export function createExtension(name, options) {
  return statement(native.ddlCreateExtension(name, options));
}

/**
 * @param {string} name
 * @param {object} [options]
 */
export function dropExtension(name, options) {
  return statement(native.ddlDropExtension(name, options));
}
