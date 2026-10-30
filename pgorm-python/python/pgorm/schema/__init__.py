"""Explicit native DDL builders and schema generation for registered Rust entities.

Construction performs no database work. Execute statements explicitly through
``await pool.execute(statement)`` or ``await connection.execute(statement)``.
"""

from .._native import (
    DataType as DataType, ColumnDef as ColumnDef, DDL as DDL,
    CreateTable as CreateTable, CreateIndex as CreateIndex,
    EntitySchema as EntitySchema, schema_from_entity as from_entity,
    drop_table as drop_table, rename_table as rename_table,
    rename_column as rename_column, truncate_table as truncate_table,
    add_column as add_column, modify_column as modify_column,
    drop_column as drop_column, set_expression as set_expression,
    drop_expression as drop_expression, add_primary_key as add_primary_key,
    add_unique as add_unique, add_not_null as add_not_null,
    add_check as add_check, validate_constraint as validate_constraint,
    alter_constraint as alter_constraint, drop_constraint as drop_constraint,
    rename_constraint as rename_constraint, drop_index as drop_index,
    create_enum as create_enum, add_enum_value as add_enum_value,
    rename_enum as rename_enum, rename_enum_value as rename_enum_value,
    drop_enum as drop_enum,
    create_view as create_view, create_recursive_view as create_recursive_view,
    drop_view as drop_view, create_materialized_view as create_materialized_view,
    refresh_materialized_view as refresh_materialized_view,
    drop_materialized_view as drop_materialized_view, create_table_as as create_table_as,
    create_schema as create_schema, drop_schema as drop_schema, rename_schema as rename_schema,
    CreateDomain as CreateDomain, set_domain_default as set_domain_default,
    drop_domain_default as drop_domain_default, set_domain_not_null as set_domain_not_null,
    drop_domain_not_null as drop_domain_not_null, add_domain_check as add_domain_check,
    drop_domain_constraint as drop_domain_constraint,
    rename_domain_constraint as rename_domain_constraint,
    validate_domain_constraint as validate_domain_constraint, rename_domain as rename_domain,
    drop_domain as drop_domain,
)

create_table = CreateTable
create_index = CreateIndex
create_domain = CreateDomain

__all__ = [
    "DataType", "ColumnDef", "DDL", "CreateTable", "CreateIndex", "EntitySchema",
    "create_table", "create_index", "from_entity", "drop_table", "rename_table",
    "rename_column", "truncate_table", "add_column", "modify_column", "drop_column",
    "set_expression", "drop_expression",
    "add_primary_key", "add_unique", "add_not_null", "add_check", "validate_constraint",
    "alter_constraint", "drop_constraint", "rename_constraint", "drop_index", "create_enum",
    "add_enum_value",
    "rename_enum",
    "rename_enum_value", "drop_enum",
    "create_view", "create_recursive_view", "drop_view", "create_materialized_view",
    "refresh_materialized_view", "drop_materialized_view", "create_table_as",
    "create_schema", "drop_schema", "rename_schema",
    "CreateDomain", "create_domain", "set_domain_default", "drop_domain_default",
    "set_domain_not_null", "drop_domain_not_null", "add_domain_check", "drop_domain_constraint",
    "rename_domain_constraint", "validate_domain_constraint", "rename_domain", "drop_domain",
]
