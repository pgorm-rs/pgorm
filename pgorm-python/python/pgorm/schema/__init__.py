"""Explicit native DDL builders and schema generation for registered Rust entities.

Construction performs no database work. Execute statements explicitly through
``await pool.execute(statement)`` or ``await connection.execute(statement)``.
Every complete statement is a ``DDL``; an ``ALTER`` that names its object and
no action yet (``PendingAlterTable`` and its kin) is not, and its first action
gives the statement.
"""

# [spec:pgorm:req:bindings.schema-api]
from .._native import (
    DataType as DataType, ColumnDef as ColumnDef, DDL as DDL, EntitySchema as EntitySchema,
    schema_from_entity as from_entity,
    CreateTable as CreateTable, CreateIndex as CreateIndex, drop_table as drop_table,
    rename_table as rename_table, rename_column as rename_column,
    rename_constraint as rename_constraint, truncate_table as truncate_table,
    comment_on_table as comment_on_table, comment_on_column as comment_on_column,
    drop_index as drop_index,
    PendingAlterTable as PendingAlterTable, AlterTable as AlterTable, alter_table as alter_table,
    CreateType as CreateType, PendingAlterType as PendingAlterType,
    AlterComposite as AlterComposite, RenameAttribute as RenameAttribute,
    alter_type as alter_type, drop_type as drop_type,
    CreateSequence as CreateSequence, PendingAlterSequence as PendingAlterSequence,
    AlterSequence as AlterSequence, alter_sequence as alter_sequence,
    drop_sequence as drop_sequence, rename_sequence as rename_sequence,
    create_extension as create_extension, drop_extension as drop_extension,
    CreateView as CreateView, CreateRecursiveView as CreateRecursiveView,
    CreateMaterializedView as CreateMaterializedView, CreateTableAs as CreateTableAs,
    drop_view as drop_view, refresh_materialized_view as refresh_materialized_view,
    drop_materialized_view as drop_materialized_view,
    create_schema as create_schema, drop_schema as drop_schema, rename_schema as rename_schema,
    CreateDomain as CreateDomain, PendingAlterDomain as PendingAlterDomain,
    alter_domain as alter_domain, drop_domain as drop_domain,
)

create_table = CreateTable
create_index = CreateIndex
create_type = CreateType
create_domain = CreateDomain
create_sequence = CreateSequence
create_view = CreateView
create_recursive_view = CreateRecursiveView
create_materialized_view = CreateMaterializedView
create_table_as = CreateTableAs

__all__ = [
    "DataType", "ColumnDef", "DDL", "EntitySchema", "from_entity",
    "CreateTable", "create_table", "PendingAlterTable", "AlterTable", "alter_table",
    "drop_table", "rename_table", "rename_column", "rename_constraint", "truncate_table",
    "comment_on_table", "comment_on_column",
    "CreateIndex", "create_index", "drop_index",
    "CreateType", "create_type", "PendingAlterType", "AlterComposite", "RenameAttribute",
    "alter_type", "drop_type",
    "CreateDomain", "create_domain", "PendingAlterDomain", "alter_domain", "drop_domain",
    "CreateSequence", "create_sequence", "PendingAlterSequence", "AlterSequence",
    "alter_sequence", "drop_sequence", "rename_sequence", "create_extension", "drop_extension",
    "CreateView", "create_view", "CreateRecursiveView", "create_recursive_view",
    "CreateMaterializedView", "create_materialized_view", "CreateTableAs", "create_table_as",
    "drop_view", "refresh_materialized_view", "drop_materialized_view",
    "create_schema", "drop_schema", "rename_schema",
]
