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

use crate::{QueryBuilder, types::IntoTableName};

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
    /// Construct table [`TableCreateStatement`]
    pub fn create() -> TableCreateStatement {
        TableCreateStatement::new()
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

    /// Construct table [`TableDropStatement`]
    pub fn drop() -> TableDropStatement {
        TableDropStatement::new()
    }

    /// Construct table [`TableRenameStatement`]
    pub fn rename() -> TableRenameStatement {
        TableRenameStatement::new()
    }

    /// Construct column [`ColumnRenameStatement`]
    pub fn rename_column() -> ColumnRenameStatement {
        ColumnRenameStatement::new()
    }

    /// Construct table [`TableTruncateStatement`]
    pub fn truncate() -> TableTruncateStatement {
        TableTruncateStatement::new()
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
