"""Explicit native DDL builders and schema generation for registered Rust entities.

Construction performs no database work. Execute statements explicitly through
``await pool.execute(statement)`` or ``await connection.execute(statement)``.
"""

from .._native import (
    DataType as DataType, ColumnDef as ColumnDef, DDL as DDL,
    CreateTable as CreateTable, CreateIndex as CreateIndex,
    EntitySchema as EntitySchema, schema_from_entity as from_entity,
    drop_table as drop_table, rename_table as rename_table,
    rename_column as rename_column, truncate as truncate,
    add_column as add_column, modify_column as modify_column,
    drop_column as drop_column, drop_index as drop_index,
    create_enum as create_enum, add_enum_value as add_enum_value,
    rename_enum as rename_enum, rename_enum_value as rename_enum_value,
    drop_enum as drop_enum,
)

create_table = CreateTable
create_index = CreateIndex

__all__ = [
    "DataType", "ColumnDef", "DDL", "CreateTable", "CreateIndex", "EntitySchema",
    "create_table", "create_index", "from_entity", "drop_table", "rename_table",
    "rename_column", "truncate", "add_column", "modify_column", "drop_column",
    "drop_index", "create_enum", "add_enum_value", "rename_enum",
    "rename_enum_value", "drop_enum",
]
