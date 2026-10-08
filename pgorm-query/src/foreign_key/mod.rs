//! Foreign key definition & alternations statements.
//!
//! # Usage
//!
//! - Table Foreign Key Create, see [`ForeignKeyCreateStatement`]
//!
//! A foreign key is dropped as a constraint of any kind is, by name:
//! [`TableAlterStatement::drop_constraint`](crate::TableAlterStatement::drop_constraint).

mod common;
mod create;

pub use common::*;
pub use create::*;

use crate::types::{IntoName, IntoTableName};

/// Shorthand for constructing any foreign key statement
#[derive(Debug, Clone)]
pub struct ForeignKey;

/// All available types of foreign key statement
// Boxing a variant would change the public shape of a DDL statement enum callers match on.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum ForeignKeyStatement {
    Create(ForeignKeyCreateStatement),
}

impl ForeignKey {
    /// Construct foreign key [`ForeignKeyCreateStatement`] over the two tables
    /// it relates and the first `(column, referenced column)` pair it maps
    pub fn create<T, C, R, S>(
        table: T,
        column: C,
        ref_table: R,
        ref_column: S,
    ) -> ForeignKeyCreateStatement
    where
        T: IntoTableName,
        C: IntoName,
        R: IntoTableName,
        S: IntoName,
    {
        ForeignKeyCreateStatement::new(table, column, ref_table, ref_column)
    }
}
