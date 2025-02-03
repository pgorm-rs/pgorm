mod connection;
mod db_connection;
// mod statement;
// mod stream;
// mod transaction;

pub use connection::*;
pub use db_connection::*;
use futures::FutureExt as _;
pub use tokio_postgres::Config;

use pgorm_pool::{ClientWrapper, Manager, ManagerConfig, Pool, PoolBuilder, RecyclingMethod};
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
    sync::Arc,
};
// pub use statement::*;
// pub use stream::*;
use tokio_postgres::NoTls;
// pub use transaction::*;

/// Method to create a [DatabasePool] on a database
pub fn connect(config: Config) -> DatabasePool {
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
        tag: None,
    };
    let mgr = Manager::from_config(config, NoTls, mgr_config);
    let pool = Pool::builder(mgr).build().unwrap();

    DatabasePool(pool)
}

pub fn connect_with_builder(
    config: Config,
    build: impl Fn(PoolBuilder) -> PoolBuilder,
) -> DatabasePool {
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
        tag: None,
    };
    let mgr = Manager::from_config(config, NoTls, mgr_config);
    let builder = build(Pool::builder(mgr));
    builder.build().map(DatabasePool).unwrap()
}

pub fn connect_multi_with_builder(
    config: Config,
    build: BTreeMap<String, Box<dyn Fn(PoolBuilder) -> PoolBuilder>>,
) -> BTreeMap<Arc<String>, DatabasePool> {
    build
        .into_iter()
        .map(|(key, build)| {
            let mgr_config = ManagerConfig {
                recycling_method: RecyclingMethod::Fast,
                tag: Some(key),
            };

            let mgr = Manager::from_config(config.clone(), NoTls, mgr_config);
            let builder = build(Pool::builder(mgr));
            let pool = builder.build().map(DatabasePool).unwrap();

            (pool.tag(), pool)
        })
        .collect()
}
