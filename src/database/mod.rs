mod connection;
mod db_connection;

pub use connection::*;
pub use db_connection::*;
pub use tokio_postgres::{Config, IsolationLevel};

/// The pool and manager knobs [`connect_with`] takes, re-exported so shaping a
/// pool never requires naming `pgorm_pool` itself.
pub use pgorm_pool::{ManagerConfig, PoolBuilder, RecyclingMethod, StatementCacheSize};

/// The TLS connector types [`connect_with`]'s bounds are written in, plus the
/// [`NoTls`] the simple entry points pass, re-exported for the same reason.
pub use tokio_postgres::{
    NoTls, Socket,
    tls::{MakeTlsConnect, TlsConnect},
};

use crate::error::Error;
use pgorm_pool::{BuildError, Manager, Pool};
use std::{collections::BTreeMap, convert::identity, sync::Arc};

/// The builder rejected the pool it was asked to build. deadpool's only such
/// failure is a timeout configured without a runtime, so the message is carried
/// through verbatim.
fn pool_build_err(err: BuildError) -> Error {
    Error::Custom(format!("cannot build connection pool: {err}"))
}

/// The manager configuration the simple entry points delegate with: no
/// recycling query, the default statement-cache bound, and `tag` — `None`
/// everywhere but [`connect_multi_with_builder`], which names each pool.
///
/// Spelled out rather than taken from `ManagerConfig::default()` because these
/// are the defaults `conn.pool` fixes for those entry points, not whatever the
/// pool crate's own default happens to be.
fn simple_manager_config(tag: Option<String>) -> ManagerConfig {
    ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
        tag,
        statement_cache: StatementCacheSize::default(),
    }
}

/// Method to create a [DatabasePool] over every knob the pool has: the TLS
/// connector, the [`ManagerConfig`], and the [`PoolBuilder`].
///
/// This is the general entry point the other three are fixed-argument
/// shorthands for, and the only route to a pool that speaks TLS, that recycles
/// with anything but [`RecyclingMethod::Fast`], or that caches a number of
/// statements other than the default.
///
/// `tls` is a `tokio_postgres` TLS connector — [`NoTls`], or one of
/// `tokio-postgres-rustls` / `tokio-postgres-openssl`, which is where a managed
/// PostgreSQL requiring TLS is reached from.
///
/// ```no_run
/// use pgorm::{ManagerConfig, NoTls, RecyclingMethod, StatementCacheSize, connect_with};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let pool = connect_with(
///     "postgres://postgres@localhost/example".parse()?,
///     NoTls,
///     ManagerConfig {
///         recycling_method: RecyclingMethod::Verified,
///         tag: Some("reader".to_owned()),
///         statement_cache: StatementCacheSize::Disabled,
///     },
///     |builder| builder.max_size(8),
/// )?;
/// # let _ = pool;
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns [`Error::Custom`] if the builder produced by `build` cannot build a
/// pool — deadpool rejects a pool configured with timeouts but no runtime.
// [spec:pgorm:req:conn.pool+3]    the general entry point
pub fn connect_with<T>(
    config: Config,
    tls: T,
    manager: ManagerConfig,
    build: impl FnOnce(PoolBuilder) -> PoolBuilder,
) -> Result<DatabasePool, Error>
where
    T: MakeTlsConnect<Socket> + Clone + Sync + Send + 'static,
    T::Stream: Sync + Send,
    T::TlsConnect: Sync + Send,
    <T::TlsConnect as TlsConnect<Socket>>::Future: Send,
{
    let mgr = Manager::from_config(config, tls, manager);
    build(Pool::builder(mgr))
        .build()
        .map(DatabasePool)
        .map_err(pool_build_err)
}

/// Method to create a [DatabasePool] on a database
///
/// # Panics
///
/// Panics if pgorm's own default pool configuration is rejected by the builder.
/// `config` shapes the [`Manager`], not the pool, so nothing a caller supplies
/// can reach that check: a panic here is a pgorm bug, not caller error. Use
/// [`connect_with_builder`] for a fallible pool whose shape the caller chooses,
/// or [`connect_with`] for one whose TLS and manager configuration it chooses
/// too.
// [spec:pgorm:req:conn.pool+3]
pub fn connect(config: Config) -> DatabasePool {
    connect_with(config, NoTls, simple_manager_config(None), identity)
        .expect("pgorm's default pool configuration builds")
}

/// Method to create a [DatabasePool], with the pool's sizing and timeouts
/// customised by `build`.
///
/// # Errors
///
/// Returns [`Error::Custom`] if the builder produced by `build` cannot build a
/// pool — deadpool rejects a pool configured with timeouts but no runtime.
// [spec:pgorm:req:conn.pool+3]    builder customization
pub fn connect_with_builder(
    config: Config,
    build: impl Fn(PoolBuilder) -> PoolBuilder,
) -> Result<DatabasePool, Error> {
    connect_with(config, NoTls, simple_manager_config(None), build)
}

/// Method to create one [DatabasePool] per entry in `build`, each tagged with
/// that entry's key and customised by its builder closure. The returned map is
/// keyed by the resulting pool tags.
///
/// # Errors
///
/// Returns [`Error::Custom`] for the first entry whose builder cannot produce a
/// pool; no map is returned in that case.
// [spec:pgorm:sem:conn.pool.multi+2]    per-tag pool construction
pub fn connect_multi_with_builder(
    config: Config,
    build: BTreeMap<String, Box<dyn Fn(PoolBuilder) -> PoolBuilder>>,
) -> Result<BTreeMap<Arc<String>, DatabasePool>, Error> {
    build
        .into_iter()
        .map(|(key, build)| {
            let pool = connect_with(
                config.clone(),
                NoTls,
                simple_manager_config(Some(key)),
                build,
            )?;

            Ok((pool.tag(), pool))
        })
        .collect()
}
