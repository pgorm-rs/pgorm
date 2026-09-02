mod connection;
mod db_connection;

pub use connection::*;
pub use db_connection::*;
pub use tokio_postgres::{Config, IsolationLevel};

use pgorm_pool::{Manager, ManagerConfig, Pool, PoolBuilder, RecyclingMethod};
use std::{collections::BTreeMap, sync::Arc};
use tokio_postgres::NoTls;

/// Method to create a [DatabasePool] on a database
///
/// # Panics
///
/// Panics if the pool cannot be built from `config`.
// [spec:pgorm:req:conn.pool]
pub fn connect(config: Config) -> DatabasePool {
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
        tag: None,
    };
    let mgr = Manager::from_config(config, NoTls, mgr_config);
    let pool = Pool::builder(mgr).build().expect("build connection pool");

    DatabasePool(pool)
}

/// Method to create a [DatabasePool], with the pool's sizing and timeouts
/// customised by `build`.
///
/// # Panics
///
/// Panics if the pool cannot be built from `config` and the customised builder.
// [spec:pgorm:req:conn.pool]    builder customization
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
    builder
        .build()
        .map(DatabasePool)
        .expect("build connection pool")
}

/// Method to create one [DatabasePool] per entry in `build`, each tagged with
/// that entry's key and customised by its builder closure. The returned map is
/// keyed by the resulting pool tags.
///
/// # Panics
///
/// Panics if any of the pools cannot be built.
// [spec:pgorm:sem:conn.pool.multi]    per-tag pool construction
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
            let pool = builder
                .build()
                .map(DatabasePool)
                .expect("build connection pool");

            (pool.tag(), pool)
        })
        .collect()
}
