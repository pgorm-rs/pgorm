// @ts-check
// @ts-self-types="./index.d.ts"

// pgorm from Node.js and Deno. This module loads the native addon, hands it
// the error and value classes it rejects with and builds values from, and is
// what an application imports.
// [spec:pgorm:def:napi.api+1]
// [spec:pgorm:req:napi.typing]

import { loadAddon } from "./native.js";
import { installConnections } from "./operations.js";
import { makeError } from "./errors.js";
import { install } from "./values.js";

// Dates and times are Temporal's, which Node.js carries from version 26.
// [spec:pgorm:req:napi.runtimes+1]
if (typeof globalThis.Temporal !== "object") {
  throw new Error(
    "pgorm-napi needs a runtime with Temporal as a global: Node.js 26 or later, or a Deno that carries it",
  );
}

const native = loadAddon();

native.setErrorFactory(makeError);
install(native);
installConnections(native);

export { connect, Connection, Pool, Queryable, Transaction } from "./connections.js";
export { RowStream } from "./streams.js";
export { CaseOperand, caseOf, caseWhen, Condition, SearchedCase, SimpleCase } from "./conditions.js";
export { Aliased, bind, call, col, exists, Expr, OrderBy, scalar, tuple } from "./expressions.js";
export { Builder } from "./builder.js";
export { FromItem, Select, select, Table, With } from "./select.js";
export { Delete, deleteFrom, Insert, insert, ReturningRow, Update, update } from "./writes.js";
export { Conflict, ConflictTarget, ConflictUpdate } from "./conflicts.js";
export { Merge, merge, MergeAction, MergeInsert, MergeUpdate, PendingMerge } from "./merge.js";
export { DataType } from "./data-type.js";
export {
  formatJson,
  isJson,
  isNotJson,
  jsonArray,
  jsonArrayAgg,
  jsonArrayQuery,
  JsonDefault,
  jsonDefault,
  jsonExists,
  JsonInput,
  jsonObject,
  jsonObjectAgg,
  jsonParse,
  jsonQuery,
  jsonScalar,
  jsonSerialize,
  jsonValue,
} from "./json.js";
export { jsonTable, JsonTableColumn } from "./json-table.js";
export {
  Frame,
  FrameCurrentRowStart,
  FrameFollowingStart,
  FramePrecedingStart,
  FrameType,
  Window,
  Windowed,
  WindowFunction,
  windowFunction,
} from "./windows.js";
export { Column, column, ModelColumn } from "./model-columns.js";
export { Model, model } from "./models.js";
export { ModelQuery } from "./model-queries.js";
export { ModelDelete, ModelInsert, ModelRows, ModelUpdate } from "./model-writes.js";
export { Relation } from "./relations.js";
export { Graph } from "./graphs.js";
export { Cursor, Paginator } from "./cursors.js";
export { ActiveModel, entities, Entity, entity } from "./entities.js";
export { EntityColumn, EntityQuery } from "./entity-queries.js";
export { EntityInsert, EntityUpdate, EntityUpdateMany } from "./entity-versions.js";
export { EntityGraph, EntityGraphCursor, EntityGraphQuery, graph, graphs } from "./entity-graphs.js";
export {
  ColumnDef,
  commentOnColumn,
  commentOnTable,
  createTable,
  CreateTable,
  dropTable,
  renameColumn,
  renameConstraint,
  renameTable,
  SchemaStatement,
  truncateTable,
} from "./schema.js";
export { AlterTable, alterTable, PendingAlterTable } from "./schema-alter.js";
export {
  AlterComposite,
  alterType,
  createType,
  CreateType,
  dropType,
  PendingAlterType,
  RenameAttribute,
} from "./schema-types.js";
export {
  AlterSequence,
  alterSequence,
  createExtension,
  createIndex,
  CreateIndex,
  createSequence,
  CreateSequence,
  dropExtension,
  dropIndex,
  dropSequence,
  PendingAlterSequence,
  renameSequence,
} from "./schema-objects.js";
export * as pipeline from "./pipeline.js";
export {
  ConnectionError,
  ConstructionError,
  DatabaseError,
  DecodeError,
  InternalError,
  LifecycleError,
  PgormError,
  TimeoutError,
} from "./errors.js";
export {
  CreatedMultirange,
  CreatedRange,
  Decimal,
  Interval,
  Multirange,
  Range,
  TypeName,
  Uuid,
  Value,
} from "./values.js";

/** @type {string} */
export const version = native.version;
