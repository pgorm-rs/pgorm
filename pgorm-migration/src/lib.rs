#![deny(rust_2018_idioms)]

pub mod migrator;
pub mod prelude;
pub mod seaql_migrations;
pub mod util;

pub use migrator::*;

pub use async_trait;
pub use pgorm;
use pgorm::DatabaseTransaction;
pub use pgorm::DbErr;
pub use pgorm::pgorm_query;

// [spec:pgorm:sem:migration.name+1]    ledger identity, normally the file stem
pub trait MigrationName {
    fn name(&self) -> &str;
}

/// The migration definition
// [spec:pgorm:def:migration.runner]    author-facing half
// [spec:pgorm:req:migration.up-only]    `up` is the only direction
#[async_trait::async_trait]
pub trait MigrationTrait: MigrationName + Send + Sync {
    /// Define actions to perform when applying the migration
    async fn up(&self, tx: &DatabaseTransaction<'_>) -> Result<(), DbErr>;
}
