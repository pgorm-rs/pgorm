// @ts-check

// CREATE TYPE — a shell type, an enumeration, a composite or a range — ALTER
// TYPE over pgorm-query's typestate, and DROP TYPE.
// [spec:pgorm:req:napi.schema-types]

import { arg, Handle, trusted, TRUSTED } from "./builder.js";
import { native } from "./operations.js";
import { SchemaStatement, statement } from "./schema.js";

/**
 * @typedef {string | import("./values.js").TypeName} TypeRef
 * @typedef {unknown} ColumnType
 */

/**
 * `CREATE TYPE`: a shell type until a kind is chosen. What a type is, is one
 * slot, so choosing a kind replaces the one before.
 */
export class CreateType extends SchemaStatement {
  /** An enumeration, with the labels `values` appends. */
  asEnum() {
    return new CreateType(native.ddlTypeAsEnum(arg(this)), TRUSTED);
  }

  /**
   * Labels appended to an enumeration, which the type becomes.
   *
   * @param {readonly string[]} labels
   */
  values(labels) {
    return new CreateType(native.ddlTypeValues(arg(this), labels), TRUSTED);
  }

  /** A composite, with the attributes `attribute` appends. */
  asComposite() {
    return new CreateType(native.ddlTypeAsComposite(arg(this)), TRUSTED);
  }

  /**
   * An attribute appended to a composite, which the type becomes.
   *
   * @param {string} name
   * @param {ColumnType} type
   * @param {object} [options]
   */
  attribute(name, type, options) {
    return new CreateType(native.ddlTypeAttribute(arg(this), name, arg(type), options), TRUSTED);
  }

  /**
   * @param {ColumnType} subtype
   * @param {object} [options]
   */
  asRange(subtype, options) {
    return new CreateType(native.ddlTypeAsRange(arg(this), arg(subtype), options), TRUSTED);
  }
}

/** @param {TypeRef} name the type, a TypeName giving its schema */
export function createType(name) {
  return new CreateType(native.ddlCreateType(name), TRUSTED);
}

/** @type {(handle: unknown) => AlterComposite} */
let compositeOf;

/**
 * The attribute changes a composite's `ALTER TYPE` takes, before its first
 * and after it alike.
 *
 * @template {new (...args: any[]) => Handle} B
 * @param {B} Base
 */
function attributeChanges(Base) {
  return class extends Base {
    /**
     * @param {string} name
     * @param {ColumnType} type
     * @param {object} [options]
     */
    addAttribute(name, type, options) {
      return compositeOf(native.ddlCompositeAdd(arg(this), name, arg(type), options));
    }

    /**
     * @param {string} name
     * @param {{ ifExists?: boolean }} [options]
     */
    dropAttribute(name, options) {
      return compositeOf(native.ddlCompositeDrop(arg(this), name, options));
    }

    /**
     * @param {string} name
     * @param {ColumnType} type
     * @param {object} [options]
     */
    alterAttribute(name, type, options) {
      return compositeOf(native.ddlCompositeAlter(arg(this), name, arg(type), options));
    }
  };
}

/** An `ALTER TYPE` before its change, which PostgreSQL cannot parse. */
export class PendingAlterType extends attributeChanges(Handle) {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "PendingAlterType", "alterType(name)");
    super(handle);
  }

  /**
   * @param {string} label
   * @param {{ before?: string, after?: string }} [options]
   */
  addValue(label, options) {
    return statement(native.ddlTypeAddValue(arg(this), label, options));
  }

  /** @param {string} name */
  renameTo(name) {
    return statement(native.ddlTypeRenameTo(arg(this), name));
  }

  /**
   * @param {string} label
   * @param {string} newLabel
   */
  renameValue(label, newLabel) {
    return statement(native.ddlTypeRenameValue(arg(this), label, newLabel));
  }

  /**
   * @param {string} name
   * @param {string} newName
   */
  renameAttribute(name, newName) {
    return new RenameAttribute(native.ddlTypeRenameAttribute(arg(this), name, newName), TRUSTED);
  }
}

export class AlterComposite extends attributeChanges(SchemaStatement) {
  static {
    compositeOf = (handle) => new AlterComposite(handle, TRUSTED);
  }

  /** @param {"cascade" | "restrict"} behavior */
  behavior(behavior) {
    return compositeOf(native.ddlTypeBehavior(arg(this), behavior));
  }
}

export class RenameAttribute extends SchemaStatement {
  /** @param {"cascade" | "restrict"} behavior */
  behavior(behavior) {
    return new RenameAttribute(native.ddlTypeBehavior(arg(this), behavior), TRUSTED);
  }
}

/** @param {TypeRef} name the type, a TypeName giving its schema */
export function alterType(name) {
  return new PendingAlterType(native.ddlAlterType(name), TRUSTED);
}

/**
 * @param {TypeRef | readonly TypeRef[]} names
 * @param {object} [options]
 */
export function dropType(names, options) {
  return statement(native.ddlDropType(names, options));
}
