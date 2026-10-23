#![allow(unused_imports, dead_code)]

//! Two pools on one database, as two processes would hold them: one given a
//! `statement_cache_channel` and one not. A domain's changed default is the
//! DDL a prepared insert does not see, so which default an insert writes says
//! whether its pool cleared.

pub mod common;

pub use common::{TestContext, setup::*};

use std::{future::Future, time::Duration};

use pgorm::{
    ConnectionTrait, DatabaseConnection, DatabasePool, Error, ManagerConfig, NoTls,
    RecyclingMethod, STATEMENT_CACHE_CHANNEL, StatementCacheSize, notify,
};
use pretty_assertions::assert_eq;

/// How long a change the listener makes is waited for.
const ARRIVAL: Duration = Duration::from_secs(10);

const INSERT: &str = "INSERT INTO line (id) VALUES ($1) RETURNING qty";

const SCHEMA: &str = "CREATE DOMAIN quantity AS integer DEFAULT 1; \
                      CREATE TABLE line (id integer PRIMARY KEY, qty quantity)";

const SET_DEFAULT: &str = "ALTER DOMAIN quantity SET DEFAULT 2";

fn manager(channel: Option<&str>) -> ManagerConfig {
    ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
        tag: None,
        statement_cache: StatementCacheSize::default(),
        statement_cache_channel: channel.map(str::to_owned),
    }
}

/// A one-connection pool on `ctx`'s database listening on the default
/// channel, so each checkout is the connection that ran the last insert.
fn listening_pool(ctx: &TestContext) -> Result<DatabasePool, Error> {
    let base_url = std::env::var("DATABASE_URL").expect("TestContext loaded DATABASE_URL");
    pgorm::connect_with(
        config(&base_url, ctx.db_name()),
        NoTls,
        manager(Some(STATEMENT_CACHE_CHANNEL)),
        |builder| builder.max_size(1),
    )
}

/// Poll `probe` until it answers `Some`, failing the test past [`ARRIVAL`].
async fn eventually<T, F, Fut>(what: &str, mut probe: F) -> Result<T, Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<T>, Error>>,
{
    let deadline = tokio::time::Instant::now() + ARRIVAL;
    loop {
        if let Some(answer) = probe().await? {
            return Ok(answer);
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{what} did not happen within {ARRIVAL:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// The backend of this database's idle session whose last statement was the
/// listener's `LISTEN`, which is how it is told from the pool's connections.
async fn listener_backend(db: &DatabaseConnection) -> Result<Option<i32>, Error> {
    Ok(db
        .query_opt(
            "SELECT pid FROM pg_stat_activity WHERE datname = current_database() \
             AND query = $1 AND state = 'idle'",
            &[&format!("LISTEN \"{STATEMENT_CACHE_CHANNEL}\"")],
        )
        .await?
        .map(|row| row.get(0)))
}

/// Open `pool`'s listener and wait for its `LISTEN`, answering its backend.
async fn listening_backend(pool: &DatabasePool, watcher: &DatabasePool) -> Result<i32, Error> {
    drop(pool.get().await?);
    eventually("the listener's LISTEN", || async {
        listener_backend(&watcher.get().await?).await
    })
    .await
}

async fn prepared_inserts(db: &DatabaseConnection) -> Result<i64, Error> {
    Ok(db
        .query_one(
            "SELECT count(*) FROM pg_prepared_statements WHERE statement = $1",
            &[&INSERT],
        )
        .await?
        .get(0))
}

async fn insert(pool: &DatabasePool, id: i32) -> Result<i32, Error> {
    Ok(pool.get().await?.query_one(INSERT, &[&id]).await?.get(0))
}

/// Wait until `pool`'s connection is lent with its insert closed, then insert
/// through it.
async fn insert_once_cleared(pool: &DatabasePool, id: i32) -> Result<i32, Error> {
    eventually("the clear", || async {
        let db = pool.get().await?;
        if prepared_inserts(&db).await? > 0 {
            return Ok(None);
        }
        Ok(Some(db.query_one(INSERT, &[&id]).await?.get(0)))
    })
    .await
}

/// A notification on the channel clears the pool that listens there, and
/// only that pool: its next insert writes the new default, while the pool
/// without the setting keeps writing the default its insert was planned with.
// [spec:pgorm:req:conn.pool.statement-cache.listen/test]    a notification, whatever its payload,
// clears the listening pool's statements, and a pool without the setting keeps its own
#[pgorm_macros::test]
async fn only_the_listening_pool_writes_the_new_default() -> Result<(), Error> {
    let ctx = TestContext::new("statement_cache_listen_notified").await;
    let quiet = &ctx.db;
    quiet.get().await?.batch_execute(SCHEMA).await?;
    let listening = listening_pool(&ctx)?;
    listening_backend(&listening, quiet).await?;

    assert_eq!(insert(&listening, 1).await?, 1);
    assert_eq!(prepared_inserts(&listening.get().await?).await?, 1);
    assert_eq!(insert(quiet, 2).await?, 1);

    let db = quiet.get().await?;
    db.batch_execute(SET_DEFAULT).await?;
    notify(&db, STATEMENT_CACHE_CHANNEL, "the payload is not read").await?;
    drop(db);

    assert_eq!(insert_once_cleared(&listening, 3).await?, 2);
    assert_eq!(insert(quiet, 4).await?, 1);
    assert_eq!(prepared_inserts(&quiet.get().await?).await?, 1);

    drop(listening);
    ctx.delete().await;
    Ok(())
}

/// A listener whose connection is terminated reconnects and clears once, for
/// whatever was notified while it was gone, with nothing notified at all.
// [spec:pgorm:req:conn.pool.statement-cache.listen/test]    a terminated listener reconnects and
// clears the pool's statements once it listens again
#[pgorm_macros::test]
async fn a_reconnected_listener_clears_the_pool() -> Result<(), Error> {
    let ctx = TestContext::new("statement_cache_listen_reconnected").await;
    let watcher = &ctx.db;
    watcher.get().await?.batch_execute(SCHEMA).await?;
    let listening = listening_pool(&ctx)?;
    let lost = listening_backend(&listening, watcher).await?;

    assert_eq!(insert(&listening, 1).await?, 1);
    let db = watcher.get().await?;
    db.batch_execute(SET_DEFAULT).await?;
    assert_eq!(
        insert(&listening, 2).await?,
        1,
        "unnotified, the planned default stands"
    );
    db.execute("SELECT pg_terminate_backend($1)", &[&lost])
        .await?;
    drop(db);

    eventually("the reconnection", || async {
        Ok(listener_backend(&watcher.get().await?)
            .await?
            .filter(|pid| *pid != lost))
    })
    .await?;
    assert_eq!(insert_once_cleared(&listening, 3).await?, 2);

    drop(listening);
    ctx.delete().await;
    Ok(())
}

// [spec:pgorm:req:conn.pool.statement-cache.listen/test]    closing the pool closes its listener's connection
#[pgorm_macros::test]
async fn closing_the_pool_closes_its_listener() -> Result<(), Error> {
    let ctx = TestContext::new("statement_cache_listen_closed").await;
    let listening = listening_pool(&ctx)?;
    let pid = listening_backend(&listening, &ctx.db).await?;

    listening.close();
    eventually("the listener's disconnection", || async {
        let alive: bool = ctx
            .db
            .get()
            .await?
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE pid = $1)",
                &[&pid],
            )
            .await?
            .get(0);
        Ok((!alive).then_some(()))
    })
    .await?;

    drop(listening);
    ctx.delete().await;
    Ok(())
}

// [spec:pgorm:req:conn.pool.statement-cache.listen/test]    connect_with refuses an empty channel and
// one past 63 bytes, before anything connects
#[test]
fn connect_with_refuses_an_unusable_channel() {
    let config: pgorm::Config = "postgres://postgres@localhost/unreached"
        .parse()
        .expect("a valid connection string");
    let build =
        |channel: &str| pgorm::connect_with(config.clone(), NoTls, manager(Some(channel)), |b| b);

    assert!(matches!(build(""), Err(Error::Custom(_))));
    assert!(matches!(build(&"c".repeat(64)), Err(Error::Custom(_))));
    assert!(build(&"c".repeat(63)).is_ok());
    assert!(
        pgorm::connect_with(config.clone(), NoTls, manager(None), |b| b).is_ok(),
        "the setting is off by default"
    );
}
