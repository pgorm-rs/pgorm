mod connection;
mod db_connection;

pub use connection::*;
pub use db_connection::*;
pub use tokio_postgres::{Config, IsolationLevel};

use crate::error::DbErr;
use pgorm_pool::{BuildError, Manager, ManagerConfig, Pool, PoolBuilder, RecyclingMethod};
use std::{collections::BTreeMap, sync::Arc};
use tokio_postgres::NoTls;

/// The builder rejected the pool it was asked to build. deadpool's only such
/// failure is a timeout configured without a runtime, so the message is carried
/// through verbatim.
fn pool_build_err(err: BuildError) -> DbErr {
    DbErr::Custom(format!("cannot build connection pool: {err}"))
}

/// Method to create a [DatabasePool] on a database
///
/// # Panics
///
/// Panics if pgorm's own default pool configuration is rejected by the builder.
/// `config` shapes the [`Manager`], not the pool, so nothing a caller supplies
/// can reach that check: a panic here is a pgorm bug, not caller error. Use
/// [`connect_with_builder`] for a fallible pool whose shape the caller chooses.
// [spec:pgorm:req:conn.pool+1]
pub fn connect(config: Config) -> DatabasePool {
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
        tag: None,
    };
    let mgr = Manager::from_config(config, NoTls, mgr_config);
    let pool = Pool::builder(mgr)
        .build()
        .expect("pgorm's default pool configuration builds");

    DatabasePool(pool)
}

/// Method to create a [DatabasePool], with the pool's sizing and timeouts
/// customised by `build`.
///
/// # Errors
///
/// Returns [`DbErr::Custom`] if the builder produced by `build` cannot build a
/// pool — deadpool rejects a pool configured with timeouts but no runtime.
// [spec:pgorm:req:conn.pool+1]    builder customization
pub fn connect_with_builder(
    config: Config,
    build: impl Fn(PoolBuilder) -> PoolBuilder,
) -> Result<DatabasePool, DbErr> {
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
        tag: None,
    };
    let mgr = Manager::from_config(config, NoTls, mgr_config);
    build(Pool::builder(mgr))
        .build()
        .map(DatabasePool)
        .map_err(pool_build_err)
}

/// Method to create one [DatabasePool] per entry in `build`, each tagged with
/// that entry's key and customised by its builder closure. The returned map is
/// keyed by the resulting pool tags.
///
/// # Errors
///
/// Returns [`DbErr::Custom`] for the first entry whose builder cannot produce a
/// pool; no map is returned in that case.
// [spec:pgorm:sem:conn.pool.multi+1]    per-tag pool construction
pub fn connect_multi_with_builder(
    config: Config,
    build: BTreeMap<String, Box<dyn Fn(PoolBuilder) -> PoolBuilder>>,
) -> Result<BTreeMap<Arc<String>, DatabasePool>, DbErr> {
    build
        .into_iter()
        .map(|(key, build)| {
            let mgr_config = ManagerConfig {
                recycling_method: RecyclingMethod::Fast,
                tag: Some(key),
            };

            let mgr = Manager::from_config(config.clone(), NoTls, mgr_config);
            let pool = build(Pool::builder(mgr))
                .build()
                .map(DatabasePool)
                .map_err(pool_build_err)?;

            Ok((pool.tag(), pool))
        })
        .collect()
}
