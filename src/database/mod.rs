mod connection;
mod db_connection;
// mod statement;
// mod stream;
// mod transaction;

pub use connection::*;
pub use db_connection::*;
pub use tokio_postgres::Config;

use deadpool_postgres::{Manager, ManagerConfig, Pool, PoolBuilder, RecyclingMethod};
// pub use statement::*;
// pub use stream::*;
use tokio_postgres::NoTls;
// pub use transaction::*;

/// Defines a database
#[derive(Debug, Default)]
pub struct Database;

impl Database {
    /// Method to create a [DatabasePool] on a database
    // #[instrument(level = "trace", skip(config))]
    pub fn connect(config: Config) -> DatabasePool {
        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let mgr = Manager::from_config(config, NoTls, mgr_config);
        let pool = Pool::builder(mgr).build().unwrap();

        DatabasePool(pool)
    }

    pub fn with_builder(
        config: Config,
        build: impl Fn(PoolBuilder) -> PoolBuilder,
    ) -> DatabasePool {
        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let mgr = Manager::from_config(config, NoTls, mgr_config);
        let builder = build(Pool::builder(mgr));
        builder.build().map(DatabasePool).unwrap()
    }
}
