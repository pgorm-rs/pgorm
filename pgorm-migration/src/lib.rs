#![deny(rust_2018_idioms)]

pub mod ledger;
pub mod migrator;
pub mod prelude;
pub mod util;

pub use migrator::*;

pub use async_trait;
pub use pgorm;
use pgorm::DatabaseTransaction;
pub use pgorm::Error;
pub use pgorm::pgorm_query;

// [spec:pgorm:sem:migration.name+3]    ledger identity, normally the file stem
pub trait MigrationName {
    fn name(&self) -> &str;
}

/// The migration definition
// [spec:pgorm:def:migration.runner+1]    author-facing half
// [spec:pgorm:req:migration.up-only]    `up` is the only direction
#[async_trait::async_trait]
pub trait MigrationTrait: MigrationName + Send + Sync {
    /// Define actions to perform when applying the migration
    async fn up(&self, tx: &DatabaseTransaction<'_>) -> Result<(), Error>;

    /// An opaque digest of what this migration does, stored in the ledger when
    /// the migration is applied and compared against on every later run.
    ///
    /// The default is `None`: content drift is not detected for a migration
    /// that does not opt in. There is no derived answer because nothing the
    /// crate can see is a faithful stand-in for "what this migration does" —
    /// the name is already the ledger key, and hashing the source file would
    /// make a reformatted comment look like a schema change. Overriding this
    /// with a value the author controls — a hash of the DDL text, a version
    /// string bumped by hand — is what turns the check on.
    // [spec:pgorm:req:migration.checksum]    opt-in by override, `None` by default
    fn checksum(&self) -> Option<String> {
        None
    }
}
