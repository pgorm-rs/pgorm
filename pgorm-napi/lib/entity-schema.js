// @ts-check

// A registered entity's schema, written by pgorm's own entity schema path —
// `Schema::create_table_from_entity` with its enum, index and comment
// statements — as the module's DDL statements, which the terminals run as
// they run any other. A table that path refuses, such as one whose foreign
// key PostgreSQL cannot build, is a ConstructionError carrying the Rust
// message, never a schema with the key left out.
// [spec:pgorm:req:bindings.schema-from-entity]

import { arg, TRUSTED } from "./builder.js";
import { Entity } from "./entities.js";
import { native } from "./operations.js";
import { CreateTable, Ddl, statement } from "./schema.js";
import { CreateIndex } from "./schema-objects.js";
import { CreateType } from "./schema-types.js";

/** Marks a construction from within the module. */
const MADE = Symbol("pgorm-napi entity schema");

/**
 * A registered entity's schema: its table, the enum types its columns name,
 * its indexes and its comments, each in the order pgorm writes them.
 */
export class EntitySchema {
  /** @readonly @type {CreateTable} */
  table;
  /** @readonly @type {readonly CreateType[]} */
  enums;
  /** @readonly @type {readonly CreateIndex[]} */
  indexes;
  /** @readonly @type {readonly Ddl[]} */
  comments;

  /**
   * @param {[unknown, unknown[], unknown[], unknown[]]} parts
   * @param {symbol} token
   */
  constructor([table, enums, indexes, comments], token) {
    if (token !== MADE) throw new TypeError("an EntitySchema comes from fromEntity(entity)");
    this.table = new CreateTable(table, TRUSTED);
    this.enums = Object.freeze(enums.map((handle) => new CreateType(handle, TRUSTED)));
    this.indexes = Object.freeze(indexes.map((handle) => new CreateIndex(handle, TRUSTED)));
    this.comments = Object.freeze(comments.map(statement));
    Object.freeze(this);
  }
}

/**
 * The schema of a registered entity, by pgorm's own entity schema path.
 *
 * @param {Entity} entity
 */
export function fromEntity(entity) {
  if (!(entity instanceof Entity)) {
    throw new TypeError("fromEntity takes a registered Entity, from entity(name); a model's table is built with createTable");
  }
  return new EntitySchema(native.entitySchema(arg(entity)), MADE);
}
