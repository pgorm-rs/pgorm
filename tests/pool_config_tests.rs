#![allow(unused_imports, dead_code)]

pub mod common;

use std::{collections::BTreeMap, sync::Arc, time::Duration};

pub use common::{TestContext, bakery_chain::*, setup::*};
use pgorm::{ConnectionTrait, DatabasePool, entity::prelude::*};
use pgorm_pool::{PoolBuilder, PoolConfig};
use pretty_assertions::assert_eq;

fn database_url() -> String {
    dotenv::from_filename(".env.local").ok();
    dotenv::from_filename(".env").ok();
    std::env::var("DATABASE_URL").expect("Enviroment variable 'DATABASE_URL' not set")
}

/// A pool against the maintenance database, which every server has and no test
/// owns. These tests only read session state, so they need no schema.
fn maintenance_config() -> pgorm::Config {
    common::setup::config(&database_url(), "postgres")
}

async fn session_setting<C>(db: &C, name: &str) -> Result<String, DbErr>
where
    C: ConnectionTrait,
{
    Ok(db
        .query_one("SELECT current_setting($1)", &[&name])
        .await?
        .get(0))
}

// [spec:pgorm:req:conn.pool/test]    connect() builds an untagged, default-sized, Fast-recycling pool
#[pgorm_macros::test]
pub async fn connect_builds_default_fast_pool() -> Result<(), DbErr> {
    let pool = pgorm::connect(maintenance_config());

    let status = pool.status();
    assert_eq!(
        status.max_size,
        PoolConfig::default().max_size,
        "connect() takes the default deadpool pool configuration"
    );
    assert_eq!(status.size, 0);
    assert_eq!(status.available, 0);

    assert!(
        pool.tag().starts_with("default-"),
        "an untagged manager falls back to a generated tag, got {:?}",
        pool.tag()
    );

    // RecyclingMethod::Fast runs no cleanup query, so session state set on one
    // checkout is still there when the same connection is handed back out.
    let conn = pool.get().await?;
    conn.batch_execute("SET application_name = 'pgorm-connect-fast'")
        .await?;
    drop(conn);

    let conn = pool.get().await?;
    assert_eq!(
        session_setting(&conn, "application_name").await?,
        "pgorm-connect-fast"
    );
    assert_eq!(pool.status().size, 1, "the one connection was reused");

    Ok(())
}

// [spec:pgorm:req:conn.pool/test]    connect_with_builder applies the closure before the pool is built
#[pgorm_macros::test]
pub async fn connect_with_builder_applies_closure() -> Result<(), DbErr> {
    let pool = pgorm::connect_with_builder(maintenance_config(), |builder| builder.max_size(2));

    assert_eq!(pool.status().max_size, 2);

    let first = pool.get().await?;
    let second = pool.get().await?;
    assert_eq!(pool.status().available, 0);

    let third = tokio::time::timeout(Duration::from_millis(250), pool.get()).await;
    assert!(
        third.is_err(),
        "a third checkout must block: the closure's max_size is the pool's real cap"
    );

    drop(first);

    let third = tokio::time::timeout(Duration::from_millis(250), pool.get())
        .await
        .expect("a released connection must unblock the waiter")?;
    assert_eq!(third.query_one("SELECT 1", &[]).await?.get::<_, i32>(0), 1);

    drop(second);

    Ok(())
}

// [spec:pgorm:req:conn.pool/test]    a pool that cannot be built panics rather than returning DbErr
#[pgorm_macros::test]
pub async fn connect_with_builder_panics_on_build_error() -> Result<(), DbErr> {
    // deadpool refuses to build a pool with timeouts but no runtime, and both
    // constructors unwrap() the builder result.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pgorm::connect_with_builder(maintenance_config(), |builder| {
            builder.wait_timeout(Some(Duration::from_secs(1)))
        })
    }));
    std::panic::set_hook(previous);

    assert!(
        built.is_err(),
        "pool construction failure is a panic, not a DbErr"
    );

    Ok(())
}

fn pool_for<'a>(
    pools: &'a BTreeMap<Arc<String>, DatabasePool>,
    tag: &str,
) -> Option<&'a DatabasePool> {
    pools
        .iter()
        .find(|(key, _)| key.as_str() == tag)
        .map(|(_, pool)| pool)
}

// [spec:pgorm:sem:conn.pool.multi/test]    one pool per builder entry, tagged and keyed by its map key
#[pgorm_macros::test]
pub async fn connect_multi_builds_tagged_pools() -> Result<(), DbErr> {
    let mut builders: BTreeMap<String, Box<dyn Fn(PoolBuilder) -> PoolBuilder>> = BTreeMap::new();
    builders.insert("reader".to_owned(), Box::new(|builder| builder.max_size(2)));
    builders.insert("writer".to_owned(), Box::new(|builder| builder.max_size(3)));

    let pools = pgorm::connect_multi_with_builder(maintenance_config(), builders);

    assert_eq!(
        pools.keys().map(|key| key.as_str()).collect::<Vec<_>>(),
        vec!["reader", "writer"],
        "the result is keyed by tag"
    );

    for (key, pool) in &pools {
        assert_eq!(&pool.tag(), key, "each pool carries its own map key as tag");
    }

    let reader = pool_for(&pools, "reader").expect("reader pool");
    let writer = pool_for(&pools, "writer").expect("writer pool");

    assert_eq!(
        reader.status().max_size,
        2,
        "each entry's own builder shapes its own pool"
    );
    assert_eq!(writer.status().max_size, 3);

    // Selection is explicit by tag only: an unknown key resolves to nothing,
    // there is no fallback onto a sibling pool.
    assert!(pool_for(&pools, "missing").is_none());

    // Every pool clones the one shared config, so they reach the same database.
    for pool in pools.values() {
        let conn = pool.get().await?;
        let database: String = conn
            .query_one("SELECT current_database()", &[])
            .await?
            .get(0);
        assert_eq!(database, "postgres");
    }

    // Each pool uses RecyclingMethod::Fast, which runs no cleanup query.
    let conn = reader.get().await?;
    conn.batch_execute("SET application_name = 'pgorm-multi-fast'")
        .await?;
    drop(conn);

    let conn = reader.get().await?;
    assert_eq!(
        session_setting(&conn, "application_name").await?,
        "pgorm-multi-fast"
    );

    Ok(())
}
