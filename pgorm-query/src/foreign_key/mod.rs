//! Foreign key definition & alternations statements.
//!
//! # Usage
//!
//! - Table Foreign Key Create, see [`ForeignKeyCreateStatement`]
//! - Table Foreign Key Drop, see [`ForeignKeyDropStatement`]

mod common;
mod create;
mod drop;

pub use common::*;
pub use create::*;
pub use drop::*;

use crate::types::{IntoIden, IntoTableName};

/// Shorthand for constructing any foreign key statement
#[derive(Debug, Clone)]
pub struct ForeignKey;

/// All available types of foreign key statement
// Boxing a variant would change the public shape of a DDL statement enum callers match on.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum ForeignKeyStatement {
    Create(ForeignKeyCreateStatement),
    Drop(ForeignKeyDropStatement),
}

impl ForeignKey {
    /// Construct foreign key [`ForeignKeyCreateStatement`]
    pub fn create() -> ForeignKeyCreateStatement {
        ForeignKeyCreateStatement::new()
    }

    /// Construct foreign key [`ForeignKeyDropStatement`] over its table and constraint
    pub fn drop<T, N>(table: T, name: N) -> ForeignKeyDropStatement
    where
        T: IntoTableName,
        N: IntoIden,
    {
        ForeignKeyDropStatement::new(table, name)
    }
}
