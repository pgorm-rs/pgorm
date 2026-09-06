//! An async, dynamic ORM for PostgreSQL, built on tokio-postgres and deadpool.
//!
//! Entities are declared as Rust types, queries are composed through the
//! [`pgorm_query`] builder and executed with their values bound as parameters
//! rather than interpolated.
//!
//! # Renaming the dependency
//!
//! Every derive expands to call-site-relative `pgorm::...` paths, so the name
//! `pgorm` has to resolve where the derive is written. Renaming the dependency
//! in `Cargo.toml` (`my_orm = { package = "pgorm", .. }`) therefore breaks the
//! derives with a loud `E0433` naming the unresolved `pgorm`. Restore the name
//! with an alias in the module the entities live in, or at the crate root:
//!
//! ```ignore
//! use my_orm as pgorm;
//! ```
//!
//! Renaming `pgorm-query` needs nothing — the derives reach it through this
//! crate's [`pgorm_query`] re-export. `pgorm-migration` is subject to the same
//! rule as `pgorm`, since `DeriveMigrationName` names `pgorm_migration`.
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
/// A PRQL-shaped pipeline query API, compiled through prqlc
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
    DeriveRelation, DeriveValueType, FromJsonQueryResult, FromQueryResult,
};
#[cfg(feature = "macros")]
pub use tokio_postgres::row::RowIndex;

/// Hold a raw SQL string literal to the PostgreSQL grammar at compile time,
/// expanding to the literal unchanged.
///
/// libpg_query — the PostgreSQL server's own parser, which performs the
/// check — is compiled into every build regardless, since `SelectorRaw`
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
// [spec:pgorm:def:macros.sql+2]
pub use pgorm_sql_macro::sql;

/// Compile a PRQL string literal to PostgreSQL SQL at build time, expanding
/// to `(&'static str, Values)` for the raw-SQL entry points.
///
/// The text sibling of [`pipeline`](crate::pipeline): the same prqlc
/// compiler, the same libpg_query oracle over what it emits, but for
/// queries known whole at compile time. The arguments after the literal are
/// converted via `Into<`[`Value`](crate::Value)`>` in placeholder order,
/// and the macro refuses at compile time any PRQL prqlc rejects, any
/// emitted SQL the PostgreSQL grammar rejects, an argument count that does
/// not match the `$N` placeholders, or a gap in their numbering.
///
/// ```
/// use pgorm::prql;
///
/// let min_total = 5_i64;
/// let (sql, values) = prql!("from invoice | filter total > $1 | take 5", min_total);
/// assert_eq!(sql, "SELECT * FROM invoice WHERE total > $1 LIMIT 5");
/// assert_eq!(values.0.len(), 1);
/// ```
///
/// The result lands on the same escape hatches as [`sql!`](crate::sql):
///
/// ```no_run
/// # use pgorm::{prql, DecodeRaw, FromQueryResult};
/// # #[derive(FromQueryResult)]
/// # struct Invoice { total: i64 }
/// # async fn demo(db: pgorm::DatabaseConnection) -> Result<(), pgorm::Error> {
/// let (sql, values) = prql!("from invoice | filter total > $1 | take 5", 100_i64);
/// let rows: Vec<Invoice> = (sql, values).into_model::<Invoice>().all(&db).await?;
/// # Ok(())
/// # }
/// ```
// [spec:pgorm:def:macros.prql]
pub use pgorm_sql_macro::prql;

pub use pgorm_query;
pub use pgorm_query::{AliasName, Iden, Values, alias};

pub use pgorm_macros::EnumIter;
pub use strum;

pub use tokio_postgres::types;
