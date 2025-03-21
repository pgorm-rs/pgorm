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
// /// Holds types and methods to perform metric collection
// pub mod metric;
/// Holds types and methods to perform queries
pub mod query;
/// Holds types that defines the schemas of an Entity
pub mod schema;
#[doc(hidden)]
// #[cfg(all(feature = "macros", feature = "tests-cfg"))]
// pub mod tests_cfg;
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

pub use pgorm_query;
pub use pgorm_query::Iden;

pub use pgorm_macros::EnumIter;
pub use strum;

pub use tokio_postgres::types;
