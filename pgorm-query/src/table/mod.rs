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

use crate::types::{IntoIden, IntoTableName};

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
// [spec:pgorm:req:sql.ddl+5]
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

// [spec:pgorm:req:sql.ddl+5] (the wrapper dispatches the one rendering to its variant)
impl std::fmt::Display for TableStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Create(stat) => stat.fmt(f),
            Self::Alter(stat) => stat.fmt(f),
            Self::Drop(stat) => stat.fmt(f),
            Self::Rename(stat) => stat.fmt(f),
            Self::RenameColumn(stat) => stat.fmt(f),
            Self::Truncate(stat) => stat.fmt(f),
        }
    }
}
