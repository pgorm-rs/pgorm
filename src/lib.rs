//! An async, dynamic ORM for PostgreSQL, built on tokio-postgres and deadpool.
//!
//! Entities are declared as Rust types, queries are composed through the
//! [`pgorm_query`] builder and executed with their values bound as parameters
//! rather than interpolated.
#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs)]
#![deny(
    missing_debug_implementations,
    clippy::missing_panics_doc,
    clippy::unwrap_used,
    clippy::print_stderr,
    clippy::print_stdout
)]

mod database;
mod docs;
/// Module for the Entity type and operations
pub mod entity;
/// Error types for all database operations
pub mod error;
/// This module performs execution of queries on a Model or ActiveModel
mod executor;
/// Holds types and methods to perform metric collection
pub mod metric;
/// A PRQL-shaped pipeline query API (off-by-default `pipeline` feature)
pub mod pipeline;
/// Holds types and methods to perform queries
pub mod query;
/// Holds types that defines the schemas of an Entity
pub mod schema;
#[doc(hidden)]
#[cfg(all(feature = "macros", feature = "tests-cfg"))]
pub mod tests_cfg;
mod util;

pub use database::*;
pub use entity::*;
pub use error::*;
pub use executor::*;
pub use query::*;
pub use schema::*;

#[cfg(feature = "macros")]
pub use pgorm_macros::{
    DeriveActiveEnum, DeriveActiveModel, DeriveActiveModelBehavior, DeriveColumn,
    DeriveCustomColumn, DeriveDisplay, DeriveEntity, DeriveEntityModel, DeriveIden,
    DeriveIntoActiveModel, DeriveMigrationName, DeriveModel, DerivePartialModel, DerivePrimaryKey,
    DeriveRelatedEntity, DeriveRelation, DeriveValueType, FromJsonQueryResult, FromQueryResult,
};
#[cfg(feature = "macros")]
pub use tokio_postgres::row::RowIndex;

/// Hold a raw SQL string literal to the PostgreSQL grammar at compile time,
/// expanding to the literal unchanged.
///
/// Available under the off-by-default `sql-macro` feature, which brings the
/// macro into scope. libpg_query — the PostgreSQL server's own parser, which
/// performs the check — is compiled either way, since `SelectorRaw`
/// pagination parses raw statements with it at runtime. The check is syntax
/// only: unknown tables and columns pass.
///
/// ```
/// # use pgorm::sql;
/// const BY_ID: &str = sql!(r#"SELECT "id", "name" FROM "cake" WHERE "id" = $1"#);
/// ```
///
/// The escape hatches that take SQL as text are its call sites:
/// [`SelectorRaw::from_statement`](crate::SelectorRaw::from_statement),
/// [`ConnectionTrait::query_raw`](crate::ConnectionTrait::query_raw),
/// [`ConnectionTrait::execute_raw`](crate::ConnectionTrait::execute_raw), and
/// [`ConnectionTrait::batch_execute`](crate::ConnectionTrait::batch_execute).
// [spec:pgorm:def:macros.sql+1]
#[cfg(feature = "sql-macro")]
pub use pgorm_sql_macro::sql;

pub use pgorm_query;
pub use pgorm_query::Iden;

pub use pgorm_macros::EnumIter;
pub use strum;

pub use tokio_postgres::types;
