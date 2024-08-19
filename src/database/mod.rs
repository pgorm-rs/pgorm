use std::time::Duration;

mod connection;
mod db_connection;
// mod statement;
// mod stream;
// mod transaction;

pub use connection::*;
pub use db_connection::*;
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
// pub use statement::*;
use std::borrow::Cow;
// pub use stream::*;
use tokio_postgres::NoTls;
use tracing::instrument;
// pub use transaction::*;

use crate::error::*;

/// Defines a database
#[derive(Debug, Default)]
pub struct Database;

impl Database {
    /// Method to create a [DatabasePool] on a database
    // #[instrument(level = "trace", skip(config))]
    pub fn connect(config: tokio_postgres::Config) -> Result<DatabasePool, DbErr> {
        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let mgr = Manager::from_config(config, NoTls, mgr_config);
        let pool = Pool::builder(mgr).max_size(16).build().unwrap();

        Ok(DatabasePool(pool))
    }
}
