/**
 * DDL over pgorm-query's builders: tables, their columns, keys and
 * constraints and their alterations, indexes, types, sequences, extensions and
 * comments.
 *
 * A DDL statement takes no parameters, so each renders with every value it
 * carries — a column's `DEFAULT`, a `CHECK`'s operands — written as
 * pgorm-query's escaped literal, and runs through `execute` as SQL text.
 * `inspect()` gives that SQL and no values. Constructing a statement does no
 * I/O; nothing runs DDL but a terminal.
 *
 * @module
 */

// [spec:pgorm:req:napi.typing]
// [spec:pgorm:req:napi.schema]

import type { CreatedMultirange, CreatedRange, TypeName } from "./index.d.ts";
import type { Builder, Expr, Operand, Predicate } from "./expressions.d.ts";
import type { BuiltinType, DataType } from "./json.d.ts";
import type { Table } from "./select.d.ts";

/**
 * A table or sequence a statement names: a name, or a {@link Table} without
 * an alias, whose schema qualifies it.
 */
export type Relation = string | Table;

/** A type a statement names: a name, or a {@link TypeName}, whose schema qualifies it. */
export type TypeRef = string | TypeName;

export type ColumnType = DataType | BuiltinType | TypeName | CreatedRange | CreatedMultirange;

export type Collation = string | { readonly name: string; readonly schema?: string };

/** One column, or a non-empty list of them in order. */
export type Columns = string | readonly [string, ...string[]];

/** When a constraint is checked: row by row, at each statement's end, or at commit. */
export type Deferrability = "notDeferrable" | "deferrableInitiallyImmediate" | "deferrableInitiallyDeferred";

/** PostgreSQL 18's `ENFORCED` / `NOT ENFORCED`; a `NOT ENFORCED` constraint is recorded and never checked. */
export type Enforcement = "enforced" | "notEnforced";

/** What a drop does with the objects that depend on what it drops. */
export type DropBehavior = "restrict" | "cascade";

export type ReferentialAction = "noAction" | "restrict" | "cascade" | "setNull" | "setDefault";

/**
 * A sequence's options, a standalone sequence's and an identity column's
 * alike: integers within `bigint`, a bound's `null` being its `NO` form.
 */
export interface SequenceOptions {
  readonly incrementBy?: number | bigint;
  readonly minValue?: number | bigint | null;
  readonly maxValue?: number | bigint | null;
  readonly startWith?: number | bigint;
  readonly cache?: number | bigint;
  readonly cycle?: boolean;
}

/** A `CHECK`'s options. `noInherit` keeps it from tables that inherit this one. */
export interface CheckOptions {
  readonly name?: string;
  readonly enforcement?: Enforcement;
  readonly noInherit?: boolean;
}

/**
 * A key's options. `withoutOverlaps` names PostgreSQL 18's period column, a
 * range or multirange, written last: rows may share the other columns only
 * while their periods do not overlap.
 */
export interface KeyOptions {
  readonly name?: string;
  readonly include?: readonly string[];
  readonly deferrability?: Deferrability;
  readonly withoutOverlaps?: string;
}

/** A unique key's options, which alone take `NULLS NOT DISTINCT`. */
export interface UniqueOptions extends KeyOptions {
  readonly nullsNotDistinct?: boolean;
}

/**
 * A foreign key's options. `period` is PostgreSQL 18's `PERIOD` pair,
 * `[column, referenced column]`, written last on both sides.
 */
export interface ForeignKeyOptions {
  readonly name?: string;
  readonly onDelete?: ReferentialAction;
  readonly onUpdate?: ReferentialAction;
  readonly deferrability?: Deferrability;
  readonly enforcement?: Enforcement;
  readonly period?: readonly [string, string];
}

/** What adds a constraint to a table that has rows: `notValid` leaves them unchecked. */
export interface NotValidOption {
  readonly notValid?: boolean;
}

/** A complete DDL statement. It inspects as it runs, with no values. */
export declare class SchemaStatement extends Builder {
  protected constructor();
}

/**
 * A table's column: its name and type, then each clause in the order it is
 * added. The type is left out only for `modifyColumn`, which then does not
 * retype the column.
 */
export declare class ColumnDef {
  constructor(name: string, type?: ColumnType);
  /** The column's one `NOT NULL` constraint, named and kept from inheriting tables as asked. */
  notNull(options?: { readonly name?: string; readonly noInherit?: boolean }): ColumnDef;
  null(): ColumnDef;
  /** `DEFAULT`: an expression, or a value written as an escaped literal. */
  default(value: Operand): ColumnDef;
  check(condition: Expr, options?: CheckOptions): ColumnDef;
  /** The kind is always named: PostgreSQL 17 refuses a generated column without one, and 18 reads it as virtual. */
  generated(expression: Expr, kind: "stored" | "virtual"): ColumnDef;
  /** The form PostgreSQL recommends over the serial family; its sequence takes a standalone sequence's options. */
  identity(generation: "always" | "byDefault", options?: SequenceOptions): ColumnDef;
  /** The serial family in place of an integer type; prefer {@link ColumnDef.identity}. */
  autoIncrement(): ColumnDef;
  collate(collation: string, options?: { readonly schema?: string }): ColumnDef;
}

/** `CREATE TABLE`; every method returns a new statement. */
export declare class CreateTable extends SchemaStatement {
  /** A column, which needs its type. */
  column(column: ColumnDef): CreateTable;
  ifNotExists(): CreateTable;
  /** The table's one primary key; a later call replaces it. */
  primaryKey(columns: Columns, options?: KeyOptions): CreateTable;
  unique(columns: Columns, options?: UniqueOptions): CreateTable;
  /** Columns paired in order with as many columns of the table they reference. */
  foreignKey(columns: Columns, references: Relation, refColumns: Columns, options?: ForeignKeyOptions): CreateTable;
  check(condition: Expr, options?: CheckOptions): CreateTable;
}

export declare function createTable(table: Relation): CreateTable;

/** The actions an `ALTER TABLE` takes, before its first and after it alike. */
export interface AlterTableActions {
  addColumn(column: ColumnDef, options?: { readonly ifNotExists?: boolean }): AlterTable;
  /**
   * The column's type, nullability, default, `CHECK` or identity changed. A
   * generated expression, the serial family or a collation without a type,
   * which no such action writes, is refused.
   */
  modifyColumn(column: ColumnDef): AlterTable;
  dropColumn(name: string): AlterTable;
  addPrimaryKey(columns: Columns, options?: KeyOptions): AlterTable;
  addUnique(columns: Columns, options?: UniqueOptions): AlterTable;
  addForeignKey(
    columns: Columns,
    references: Relation,
    refColumns: Columns,
    options?: ForeignKeyOptions & NotValidOption,
  ): AlterTable;
  addCheck(condition: Expr, options?: CheckOptions & NotValidOption): AlterTable;
  /** PostgreSQL 18's table-level `NOT NULL`, the one spelling that can be `NOT VALID`. */
  addNotNull(
    column: string,
    options?: { readonly name?: string; readonly noInherit?: boolean } & NotValidOption,
  ): AlterTable;
  /** Which kind the name holds — a key, a foreign key, a `CHECK`, a `NOT NULL` — is the server's knowledge. */
  dropConstraint(name: string, options?: { readonly ifExists?: boolean; readonly behavior?: DropBehavior }): AlterTable;
  /** Checks the rows a `NOT VALID` constraint left unchecked; the server refuses any other kind. */
  validateConstraint(name: string): AlterTable;
  /** Whether a `NOT NULL` passes to inheriting tables, or whether a foreign key is enforced. */
  alterConstraint(name: string, change: "inherit" | "noInherit" | "enforced" | "notEnforced"): AlterTable;
  /** A generated column's new expression, which the rows already written take. */
  setExpression(column: string, expression: Expr): AlterTable;
  /** A stored generated column made plain, keeping its values. */
  dropExpression(column: string, options?: { readonly ifExists?: boolean }): AlterTable;
}

/**
 * An `ALTER TABLE` before its first action. PostgreSQL parses none without
 * one, so it has no `inspect()`, and a terminal refuses it with a
 * `ConstructionError`.
 */
export declare class PendingAlterTable {
  private constructor();
}
export interface PendingAlterTable extends AlterTableActions {}

/** An `ALTER TABLE` with at least one action, applied in order; every method returns a new one. */
export declare class AlterTable extends SchemaStatement {}
export interface AlterTable extends AlterTableActions {}

export declare function alterTable(table: Relation): PendingAlterTable;

export declare function dropTable(
  tables: Relation | readonly [Relation, ...Relation[]],
  options?: { readonly ifExists?: boolean; readonly behavior?: DropBehavior },
): SchemaStatement;

/** The new name is bare, since a rename cannot move a table to another schema. */
export declare function renameTable(table: Relation, name: string): SchemaStatement;

export declare function renameColumn(table: Relation, name: string, newName: string): SchemaStatement;

/** A constraint of any kind renamed, a key's index with it. */
export declare function renameConstraint(table: Relation, name: string, newName: string): SchemaStatement;

export declare function truncateTable(table: Relation): SchemaStatement;

export declare function commentOnTable(table: Relation, text: string): SchemaStatement;

export declare function commentOnColumn(table: Relation, column: string, text: string): SchemaStatement;

/**
 * An index entry: a column's name, an expression, or either with an order and
 * an operator class.
 */
export type IndexColumn =
  | string
  | Expr
  | {
    readonly on: string | Expr;
    readonly order?: "asc" | "desc";
    readonly operatorClass?: string;
  };

/** `CREATE INDEX`; every method returns a new statement. */
export declare class CreateIndex extends SchemaStatement {
  column(column: IndexColumn): CreateIndex;
  unique(): CreateIndex;
  /** `NULLS NOT DISTINCT`, which PostgreSQL defines for a unique index alone, so the index becomes one. */
  nullsNotDistinct(): CreateIndex;
  ifNotExists(): CreateIndex;
  /** The access method: `btree`, `hash`, `gin`, `gist`, `spgist`, `brin`, or another the server has. */
  using(method: string): CreateIndex;
  /** Columns carried in the index's leaves without being indexed; appends. */
  include(columns: readonly string[]): CreateIndex;
  /** The partial index's predicate, ANDed to one already there. */
  where(predicate: Predicate): CreateIndex;
}

/** An index over its first entry, named by the options or by PostgreSQL. */
export declare function createIndex(
  table: Relation,
  column: IndexColumn,
  options?: { readonly name?: string },
): CreateIndex;

/** An index dropped from its table's schema. */
export declare function dropIndex(
  table: Relation,
  name: string,
  options?: { readonly ifExists?: boolean },
): SchemaStatement;

export interface CollationOption {
  readonly collation?: Collation;
}

/**
 * `CREATE TYPE`: a shell type until a kind is chosen. What a type is, is one
 * slot, so choosing a kind replaces the one before.
 */
export declare class CreateType extends SchemaStatement {
  /** An enumeration, with no labels until `values` appends them. */
  asEnum(): CreateType;
  /** Labels appended to an enumeration, which the type becomes: data of at most 63 bytes, written as literals. */
  values(labels: readonly string[]): CreateType;
  /** A composite, with no attributes until `attribute` appends them. */
  asComposite(): CreateType;
  /** An attribute appended to a composite, which the type becomes. */
  attribute(name: string, type: ColumnType, options?: CollationOption): CreateType;
  /** A range over `subtype`, the one option PostgreSQL requires. */
  asRange(
    subtype: ColumnType,
    options?: CollationOption & {
      readonly subtypeOpclass?: string;
      readonly subtypeDiff?: string;
      readonly multirangeTypeName?: TypeRef;
    },
  ): CreateType;
}

export declare function createType(name: TypeRef): CreateType;

/** The attribute changes a composite's `ALTER TYPE` takes. */
export interface AttributeChanges {
  addAttribute(name: string, type: ColumnType, options?: CollationOption): AlterComposite;
  dropAttribute(name: string, options?: { readonly ifExists?: boolean }): AlterComposite;
  alterAttribute(name: string, type: ColumnType, options?: CollationOption): AlterComposite;
}

/**
 * An `ALTER TYPE` before its change. PostgreSQL parses none without one, so
 * it has no `inspect()`, and a terminal refuses it.
 */
export declare class PendingAlterType {
  private constructor();
  /** A label added to an enumeration: at its end, or before or after one. */
  addValue(label: string, options?: { readonly before: string } | { readonly after: string }): SchemaStatement;
  /** The type renamed; the new name is bare, the type staying in its schema. */
  renameTo(name: string): SchemaStatement;
  renameValue(label: string, newLabel: string): SchemaStatement;
  /** An attribute renamed: PostgreSQL takes `RENAME` as an `ALTER TYPE`'s sole action. */
  renameAttribute(name: string, newName: string): RenameAttribute;
}
export interface PendingAlterType extends AttributeChanges {}

/** A composite's attribute changes, applied in order; every method returns a new statement. */
export declare class AlterComposite extends SchemaStatement {
  /** `CASCADE`, which carries the changes into typed tables, or `RESTRICT`; the last call wins. */
  behavior(behavior: DropBehavior): AlterComposite;
}
export interface AlterComposite extends AttributeChanges {}

/** A composite's attribute renamed. */
export declare class RenameAttribute extends SchemaStatement {
  behavior(behavior: DropBehavior): RenameAttribute;
}

export declare function alterType(name: TypeRef): PendingAlterType;

export declare function dropType(
  names: TypeRef | readonly [TypeRef, ...TypeRef[]],
  options?: { readonly ifExists?: boolean; readonly behavior?: DropBehavior },
): SchemaStatement;

export type SequenceType = "smallint" | "integer" | "bigint";

/** The clauses a sequence statement takes, creating the sequence or altering it. */
export interface SequenceClauses<S> {
  asType(type: SequenceType): S;
  /** At least one option, merged into those set; a later one for a clause replaces the earlier. */
  options(options: SequenceOptions): S;
  /** `OWNED BY` a table's column, which takes the sequence with it when dropped. */
  ownedBy(table: Relation, column: string): S;
  /** `OWNED BY NONE`, which releases it. */
  ownedBy(table: null): S;
}

/** `CREATE SEQUENCE`; every method returns a new statement. */
export declare class CreateSequence extends SchemaStatement {
  ifNotExists(): CreateSequence;
}
export interface CreateSequence extends SequenceClauses<CreateSequence> {}

/**
 * An `ALTER SEQUENCE` before its first clause. PostgreSQL parses none
 * without one, so it has no `inspect()`, and a terminal refuses it.
 */
export declare class PendingAlterSequence {
  private constructor();
  /** `RESTART`, at `value` when one is given. */
  restart(value?: number | bigint): AlterSequence;
}
export interface PendingAlterSequence extends SequenceClauses<AlterSequence> {}

/** An `ALTER SEQUENCE` with at least one clause; every method returns a new one. */
export declare class AlterSequence extends SchemaStatement {
  ifExists(): AlterSequence;
  restart(value?: number | bigint): AlterSequence;
}
export interface AlterSequence extends SequenceClauses<AlterSequence> {}

export declare function createSequence(name: Relation): CreateSequence;

export declare function alterSequence(name: Relation): PendingAlterSequence;

export declare function dropSequence(
  names: Relation | readonly [Relation, ...Relation[]],
  options?: { readonly ifExists?: boolean; readonly behavior?: DropBehavior },
): SchemaStatement;

export declare function renameSequence(name: Relation, newName: string): SchemaStatement;

export declare function createExtension(
  name: string,
  options?: {
    readonly ifNotExists?: boolean;
    readonly schema?: string;
    readonly version?: string;
    readonly cascade?: boolean;
  },
): SchemaStatement;

export declare function dropExtension(
  name: string,
  options?: { readonly ifExists?: boolean; readonly behavior?: DropBehavior },
): SchemaStatement;
