//! A pool given a `statement_cache_channel` keeps a connection of its own
//! listening on it, and clears its statement caches on every notification
//! there and every time that connection's `LISTEN` takes effect.
//!
//! Every test runs in the shared `postgres` database, so each listens on a
//! channel named after the test and this process, which no concurrent run
//! notifies.

use std::{env, future::Future, sync::Once, time::Duration};

use pgorm_pool::{Client, ManagerConfig, Pool, PoolConfig, RecyclingMethod, StatementCacheSize};

static DOTENV: Once = Once::new();

/// How long a change the listener makes is waited for.
const ARRIVAL: Duration = Duration::from_secs(10);

const PROBE: &str = "SELECT 'cached across a clear'";

fn base_url() -> String {
    DOTENV.call_once(|| {
        dotenvy::from_filename(".env.local").ok();
        dotenvy::from_filename(".env").ok();
    });
    env::var("DATABASE_URL").expect(
        "environment variable 'DATABASE_URL' not set; it must hold a PostgreSQL server URL \
         without a database path, e.g. postgres://postgres:postgres@127.0.0.1:5432",
    )
}

fn channel(test: &str) -> String {
    format!("{test}_{}", std::process::id())
}

/// A pool of one connection, listening on `channel`, so every checkout after
/// the first is the same connection put through `Manager::recycle`.
fn listening_pool(channel: &str) -> Pool {
    let cfg = pgorm_pool::Config {
        url: Some(format!("{}/postgres", base_url())),
        manager: Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
            tag: None,
            statement_cache: StatementCacheSize::default(),
            statement_cache_channel: Some(channel.to_owned()),
        }),
        pool: Some(PoolConfig::new(1)),
        ..Default::default()
    };
    cfg.create_pool(tokio_postgres::NoTls).unwrap()
}

/// Poll `probe` until it answers `Some`, failing the test past [`ARRIVAL`].
async fn eventually<T, F, Fut>(what: &str, mut probe: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    let deadline = tokio::time::Instant::now() + ARRIVAL;
    loop {
        if let Some(answer) = probe().await {
            return answer;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{what} did not happen within {ARRIVAL:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// The backend of the session that last ran `LISTEN` on `channel` and is now
/// idle, if one is connected. `query` is a session's last statement, and it is
/// what tells the listener's backend from those of the pool's connections.
async fn listener_backend(client: &Client, channel: &str) -> Option<i32> {
    client
        .query_opt(
            "SELECT pid FROM pg_stat_activity \
             WHERE query = $1 AND state = 'idle' AND pid <> pg_backend_pid()",
            &[&format!("LISTEN \"{channel}\"")],
        )
        .await
        .unwrap()
        .map(|row| row.get(0))
}

async fn listening_backend(pool: &Pool, channel: &str) -> i32 {
    eventually("the listener's LISTEN", || async {
        listener_backend(&pool.get().await.unwrap(), channel).await
    })
    .await
}

async fn backend_alive(client: &Client, pid: i32) -> bool {
    client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE pid = $1)",
            &[&pid],
        )
        .await
        .unwrap()
        .get(0)
}

/// Cache [`PROBE`] on the pool's one connection, once the listener's first
/// clear has reached it, and show that it stays cached across a return.
async fn cache_probe(pool: &Pool) {
    let client = pool.get().await.unwrap();
    drop(client.prepare_cached(PROBE).await.unwrap());
    drop(client);
    let client = pool.get().await.unwrap();
    assert_eq!(
        client.statement_cache.size(),
        1,
        "nothing clears while no notification arrives"
    );
}

async fn cleared(pool: &Pool) -> Option<()> {
    (pool.get().await.unwrap().statement_cache.size() == 0).then_some(())
}

// [spec:pgorm:req:conn.pool.statement-cache.listen/test]    a notification on the channel, whatever
// its payload, clears the cache of every connection before it is next lent
#[tokio::test]
async fn a_notification_clears_the_cache() {
    let channel = channel("cache_listener_notified");
    let pool = listening_pool(&channel);
    listening_backend(&pool, &channel).await;
    cache_probe(&pool).await;

    pool.get()
        .await
        .unwrap()
        .execute("SELECT pg_notify($1, 'any payload')", &[&channel])
        .await
        .unwrap();
    eventually("the clear", || cleared(&pool)).await;
}

// [spec:pgorm:req:conn.pool.statement-cache.listen/test]    a terminated listener reconnects, clears
// once for the notifications it may have missed, and listens again
#[tokio::test]
async fn a_lost_listener_reconnects_and_clears() {
    let channel = channel("cache_listener_reconnected");
    let pool = listening_pool(&channel);
    let first = listening_backend(&pool, &channel).await;
    cache_probe(&pool).await;

    let terminated: bool = pool
        .get()
        .await
        .unwrap()
        .query_one("SELECT pg_terminate_backend($1)", &[&first])
        .await
        .unwrap()
        .get(0);
    assert!(terminated);

    let second = eventually("the reconnection", || async {
        listener_backend(&pool.get().await.unwrap(), &channel)
            .await
            .filter(|pid| *pid != first)
    })
    .await;
    eventually("the reconnection's clear", || cleared(&pool)).await;

    cache_probe(&pool).await;
    pool.get()
        .await
        .unwrap()
        .execute("SELECT pg_notify($1, '')", &[&channel])
        .await
        .unwrap();
    eventually("the clear after reconnecting", || cleared(&pool)).await;
    assert_eq!(listening_backend(&pool, &channel).await, second);
}

// [spec:pgorm:req:conn.pool.statement-cache.listen/test]    stopping the listener closes its
// connection while the pool lives on
#[tokio::test]
async fn stopping_the_listener_closes_its_connection() {
    let channel = channel("cache_listener_stopped");
    let pool = listening_pool(&channel);
    let pid = listening_backend(&pool, &channel).await;

    pool.manager().stop_statement_cache_listener();
    eventually("the listener's disconnection", || async {
        (!backend_alive(&pool.get().await.unwrap(), pid).await).then_some(())
    })
    .await;
}

// [spec:pgorm:req:conn.pool.statement-cache.listen/test]    dropping the pool ends the listener and
// closes its connection
#[tokio::test]
async fn dropping_the_pool_closes_the_listener() {
    let channel = channel("cache_listener_dropped");
    let pool = listening_pool(&channel);
    let pid = listening_backend(&pool, &channel).await;
    drop(pool);

    let watcher = pgorm_pool::Config {
        url: Some(format!("{}/postgres", base_url())),
        ..Default::default()
    }
    .create_pool(tokio_postgres::NoTls)
    .unwrap();
    eventually("the listener's disconnection", || async {
        (!backend_alive(&watcher.get().await.unwrap(), pid).await).then_some(())
    })
    .await;
}
