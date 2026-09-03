//! Table definition & alternations statements.
//!
//! # Usage
//!
//! - Table Create, see [`TableCreateStatement`]
//! - Table Alter, see [`TableAlterStatement`]
//! - Table Drop, see [`TableDropStatement`]
//! - Table Rename, see [`TableRenameStatement`]
//! - Column Rename, see [`ColumnRenameStatement`]
//! - Table Truncate, see [`TableTruncateStatement`]

use crate::{
    QueryBuilder,
    types::{IntoIden, IntoTableName},
};

mod alter;
mod column;
mod create;
mod drop;
mod rename;
mod truncate;

pub use alter::*;
pub use column::*;
pub use create::*;
pub use drop::*;
pub use rename::*;
pub use truncate::*;

/// Helper for constructing any table statement
// [spec:pgorm:req:sql.ddl+4]
#[derive(Debug)]
pub struct Table;

/// All available types of table statement
// Boxing a variant would change the public shape of a DDL statement enum callers match on.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum TableStatement {
    Create(TableCreateStatement),
    Alter(TableAlterStatement),
    Drop(TableDropStatement),
    Rename(TableRenameStatement),
    RenameColumn(ColumnRenameStatement),
    Truncate(TableTruncateStatement),
}

impl Table {
    /// Construct table [`TableCreateStatement`] over the table it creates
    pub fn create<T>(table: T) -> TableCreateStatement
    where
        T: IntoTableName,
    {
        TableCreateStatement::new(table)
    }

    /// Name the table a [`TableAlterStatement`] will alter.
    ///
    /// Choosing an action on the returned [`PendingTableAlter`] is what produces
    /// the statement: PostgreSQL has no spelling for an `ALTER TABLE` with no
    /// action, so naming the table alone renders nothing.
    pub fn alter<T>(table: T) -> PendingTableAlter
    where
        T: IntoTableName,
    {
        PendingTableAlter::new(table.into_table_name())
    }

    /// Construct table [`TableDropStatement`] over its first table
    pub fn drop<T>(table: T) -> TableDropStatement
    where
        T: IntoTableName,
    {
        TableDropStatement::new(table)
    }

    /// Construct table [`TableRenameStatement`] from the old and new name
    pub fn rename<T, R>(from_name: T, to_name: R) -> TableRenameStatement
    where
        T: IntoTableName,
        R: IntoIden,
    {
        TableRenameStatement::new(from_name, to_name)
    }

    /// Construct column [`ColumnRenameStatement`] over a table and two column names
    pub fn rename_column<T, F, R>(table: T, from_name: F, to_name: R) -> ColumnRenameStatement
    where
        T: IntoTableName,
        F: IntoIden,
        R: IntoIden,
    {
        ColumnRenameStatement::new(table, from_name, to_name)
    }

    /// Construct table [`TableTruncateStatement`] over the table it empties
    pub fn truncate<T>(table: T) -> TableTruncateStatement
    where
        T: IntoTableName,
    {
        TableTruncateStatement::new(table)
    }
}

impl TableStatement {
    /// Build corresponding SQL statement for certain database backend and return SQL string
    pub fn build(&self, table_builder: QueryBuilder) -> String {
        match self {
            Self::Create(stat) => stat.build(table_builder),
            Self::Alter(stat) => stat.build(table_builder),
            Self::Drop(stat) => stat.build(table_builder),
            Self::Rename(stat) => stat.build(table_builder),
            Self::RenameColumn(stat) => stat.build(table_builder),
            Self::Truncate(stat) => stat.build(table_builder),
        }
    }

    /// Build corresponding SQL statement for certain database backend and return SQL string
    pub fn build_any(&self, table_builder: &QueryBuilder) -> String {
        match self {
            Self::Create(stat) => stat.build_any(table_builder),
            Self::Alter(stat) => stat.build_any(table_builder),
            Self::Drop(stat) => stat.build_any(table_builder),
            Self::Rename(stat) => stat.build_any(table_builder),
            Self::RenameColumn(stat) => stat.build_any(table_builder),
            Self::Truncate(stat) => stat.build_any(table_builder),
        }
    }

    /// Build corresponding SQL statement for certain database backend and return SQL string
    pub fn to_string(&self, table_builder: QueryBuilder) -> String {
        match self {
            Self::Create(stat) => stat.to_string(table_builder),
            Self::Alter(stat) => stat.to_string(table_builder),
            Self::Drop(stat) => stat.to_string(table_builder),
            Self::Rename(stat) => stat.to_string(table_builder),
            Self::RenameColumn(stat) => stat.to_string(table_builder),
            Self::Truncate(stat) => stat.to_string(table_builder),
        }
    }
}
