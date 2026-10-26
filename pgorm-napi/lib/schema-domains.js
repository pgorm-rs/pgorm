// @ts-check

// CREATE DOMAIN, ALTER DOMAIN over pgorm-query's typestate, and DROP DOMAIN;
// and VALUE, the value a domain's CHECK tests.
// [spec:pgorm:req:napi.schema-domains]

import { arg, Handle, trusted, TRUSTED } from "./builder.js";
import { Expr } from "./expressions.js";
import { native } from "./operations.js";
import { Ddl, statement } from "./schema.js";

/**
 * @typedef {string | import("./values.js").TypeName} TypeRef
 * @typedef {unknown} ColumnType
 */

/**
 * `CREATE DOMAIN`: the clauses render in one order whatever order they are
 * given in, and a default and a nullability are one slot each.
 */
export class CreateDomain extends Ddl {
  /**
   * @param {string} collation
   * @param {{ schema?: string }} [options]
   */
  collate(collation, options = {}) {
    return new CreateDomain(native.ddlDomainCollate(arg(this), { name: collation, schema: options.schema }), TRUSTED);
  }

  /** @param {unknown} value */
  default(value) {
    return new CreateDomain(native.ddlDomainDefault(arg(this), arg(value)), TRUSTED);
  }

  notNull() {
    return new CreateDomain(native.ddlDomainNotNull(arg(this)), TRUSTED);
  }

  null() {
    return new CreateDomain(native.ddlDomainNull(arg(this)), TRUSTED);
  }

  /**
   * @param {Expr} condition
   * @param {{ name?: string }} [options]
   */
  check(condition, options) {
    return new CreateDomain(native.ddlDomainCheck(arg(this), arg(condition), options), TRUSTED);
  }
}

/**
 * @param {TypeRef} name the domain, a TypeName giving its schema
 * @param {ColumnType} base the type it is over
 */
export function createDomain(name, base) {
  return new CreateDomain(native.ddlCreateDomain(name, arg(base)), TRUSTED);
}

/** An `ALTER DOMAIN` before its action, which PostgreSQL cannot parse. */
export class PendingAlterDomain extends Handle {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "PendingAlterDomain", "alterDomain(name)");
    super(handle);
  }

  /** @param {unknown} value */
  setDefault(value) {
    return statement(native.ddlDomainSetDefault(arg(this), arg(value)));
  }

  dropDefault() {
    return statement(native.ddlDomainDropDefault(arg(this)));
  }

  setNotNull() {
    return statement(native.ddlDomainSetNotNull(arg(this)));
  }

  dropNotNull() {
    return statement(native.ddlDomainDropNotNull(arg(this)));
  }

  /**
   * @param {Expr} condition
   * @param {{ name?: string, notValid?: boolean }} [options]
   */
  addCheck(condition, options) {
    return statement(native.ddlDomainAddCheck(arg(this), arg(condition), options));
  }

  /**
   * @param {string} name
   * @param {{ ifExists?: boolean, behavior?: "cascade" | "restrict" }} [options]
   */
  dropConstraint(name, options) {
    return statement(native.ddlDomainDropConstraint(arg(this), name, options));
  }

  /**
   * @param {string} name
   * @param {string} newName
   */
  renameConstraint(name, newName) {
    return statement(native.ddlDomainRenameConstraint(arg(this), name, newName));
  }

  /** @param {string} name */
  validateConstraint(name) {
    return statement(native.ddlDomainValidateConstraint(arg(this), name));
  }

  /** @param {string} name */
  renameTo(name) {
    return statement(native.ddlDomainRenameTo(arg(this), name));
  }
}

/** @param {TypeRef} name the domain, a TypeName giving its schema */
export function alterDomain(name) {
  return new PendingAlterDomain(native.ddlAlterDomain(name), TRUSTED);
}

/**
 * @param {TypeRef | readonly TypeRef[]} names
 * @param {object} [options]
 */
export function dropDomain(names, options) {
  return statement(native.ddlDropDomain(names, options));
}

/** `VALUE`: inside a domain's `CHECK`, the value tested. */
export function domainValue() {
  return new Expr(native.exprDomainValue(), TRUSTED);
}
