//! Index definition & alternations statements.
//!
//! # Usage
//!
//! - Table Index Create, see [`IndexCreateStatement`]
//! - Table Index Drop, see [`IndexDropStatement`]

mod common;
mod create;
mod drop;

pub use common::*;
pub use create::*;
pub use drop::*;

use crate::types::{IntoIden, IntoTableName};

/// Shorthand for constructing any index statement
#[derive(Debug, Clone)]
pub struct Index;

/// All available types of index statement
#[derive(Debug, Clone)]
pub enum IndexStatement {
    Create(IndexCreateStatement),
    Drop(IndexDropStatement),
}

impl Index {
    /// Construct index [`IndexCreateStatement`] over its table and first column
    pub fn create<T, C>(table: T, col: C) -> IndexCreateStatement
    where
        T: IntoTableName,
        C: IntoIndexColumn,
    {
        IndexCreateStatement::new(table, col)
    }

    /// Construct index [`IndexDropStatement`] over the index it drops
    pub fn drop<T>(name: T) -> IndexDropStatement
    where
        T: IntoIden,
    {
        IndexDropStatement::new(name)
    }
}
