#[cfg(feature = "sqlx-dep")]
mod sqlx_common;
#[cfg(feature = "sqlx-postgres")]
pub(crate) mod sqlx_postgres;

#[cfg(feature = "sqlx-dep")]
pub(crate) use sqlx_common::*;
#[cfg(feature = "sqlx-postgres")]
pub use sqlx_postgres::*;
