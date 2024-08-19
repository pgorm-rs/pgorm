#![deny(rust_2018_idioms)]

pub mod manager;
pub mod migrator;
pub mod prelude;
pub mod schema;
pub mod seaql_migrations;
pub mod util;

pub use manager::*;
pub use migrator::*;

pub use async_trait;
pub use sea_orm;
pub use sea_orm::sea_query;
use sea_orm::DatabaseTransaction;
pub use sea_orm::DbErr;

pub trait MigrationName {
    fn name(&self) -> &str;
}

pub struct SchemaManager<'a>(&'a ());

/// The migration definition
#[async_trait::async_trait]
pub trait MigrationTrait: MigrationName + Send + Sync {
    /// Define actions to perform when applying the migration
    async fn up(&self, tx: &DatabaseTransaction<'_>, manager: &SchemaManager<'_>) -> Result<(), DbErr>;
}
