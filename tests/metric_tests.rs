#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, setup::*};

use async_trait::async_trait;
use pgorm::{
    ConnectionTrait, DbErr,
    metric::{
        InstrumentedConnection, InstrumentedPool, InstrumentedTransaction, LoggingMetrics,
        MetricsCollector, NoOpMetrics,
    },
};
use pretty_assertions::assert_eq;
use std::{
    fmt,
    sync::{Arc, Mutex},
    time::Duration,
};
use tracing::{
    Event, Level, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    Layer,
    layer::{Context, SubscriberExt},
    registry::LookupSpan,
    util::SubscriberInitExt,
};

#[derive(Clone, Debug, Default)]
struct RecordingMetrics {
    events: Arc<Mutex<Vec<String>>>,
}

impl RecordingMetrics {
    fn push(&self, event: String) {
        self.events.lock().unwrap().push(event);
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }

    fn count(&self, prefix: &str) -> usize {
        self.events()
            .iter()
            .filter(|event| event.starts_with(prefix))
            .count()
    }
}

#[async_trait]
impl MetricsCollector for RecordingMetrics {
    async fn record_query_success(&self, operation: &str, _duration: Duration, rows: Option<u64>) {
        self.push(format!("query_success:{operation}:{rows:?}"));
    }

    async fn record_query_error(&self, operation: &str, _duration: Duration, _error: &DbErr) {
        self.push(format!("query_error:{operation}"));
    }

    async fn record_connection_acquired(&self, _duration: Duration) {
        self.push("connection_acquired".to_owned());
    }

    async fn record_connection_error(&self, _duration: Duration, _error: &DbErr) {
        self.push("connection_error".to_owned());
    }

    async fn record_transaction_begin(&self, _duration: Duration) {
        self.push("transaction_begin".to_owned());
    }

    async fn record_transaction_commit(&self, _duration: Duration) {
        self.push("transaction_commit".to_owned());
    }

    async fn record_transaction_rollback(&self, _duration: Duration) {
        self.push("transaction_rollback".to_owned());
    }
}

// [spec:pgorm:sem:metric.layer.tx+1/test]    begin_instrumented + explicit rollback
#[pgorm_macros::test]
pub async fn instrumented_begin_and_rollback() -> Result<(), DbErr> {
    let ctx = TestContext::new("metric_layer_rollback_metrictx").await;
    let metrics = RecordingMetrics::default();
    let pool = InstrumentedPool::new(ctx.db.clone(), metrics.clone());

    let mut conn = pool.get().await?;
    conn.execute("CREATE TABLE widget (id int primary key)", &[])
        .await?;

    let txn = conn.begin_instrumented().await?;
    txn.execute("INSERT INTO widget (id) VALUES (1)", &[])
        .await?;
    assert_eq!(txn.query_all("SELECT id FROM widget", &[]).await?.len(), 1);
    txn.rollback().await?;

    assert_eq!(conn.query_all("SELECT id FROM widget", &[]).await?.len(), 0);

    assert_eq!(metrics.count("connection_acquired"), 1);
    assert_eq!(metrics.count("transaction_begin"), 1);
    assert_eq!(metrics.count("transaction_rollback"), 1);
    assert_eq!(metrics.count("transaction_commit"), 0);
    assert_eq!(metrics.count("query_error"), 0);
    assert!(
        metrics
            .events()
            .contains(&"query_success:execute:Some(1)".to_owned()),
        "statements inside the wrapped transaction stay instrumented: {:?}",
        metrics.events()
    );

    drop(conn);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:metric.layer.tx+1/test]    begin_instrumented + commit
#[pgorm_macros::test]
pub async fn instrumented_begin_and_commit() -> Result<(), DbErr> {
    let ctx = TestContext::new("metric_layer_commit_metrictx").await;
    let metrics = RecordingMetrics::default();
    let pool = InstrumentedPool::new(ctx.db.clone(), metrics.clone());

    let mut conn = pool.get().await?;
    conn.execute("CREATE TABLE widget (id int primary key)", &[])
        .await?;

    let txn = conn.begin_instrumented().await?;
    txn.execute("INSERT INTO widget (id) VALUES (1)", &[])
        .await?;
    txn.commit().await?;

    assert_eq!(conn.query_all("SELECT id FROM widget", &[]).await?.len(), 1);

    assert_eq!(metrics.count("transaction_begin"), 1);
    assert_eq!(metrics.count("transaction_commit"), 1);
    assert_eq!(metrics.count("transaction_rollback"), 0);

    drop(conn);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:req:metric.layer.delegate+2/test]    batch_execute reports a rowless success
#[pgorm_macros::test]
pub async fn instrumented_batch_execute_reports_no_rows() -> Result<(), DbErr> {
    let ctx = TestContext::new("metric_layer_batch_execute_txbatch").await;
    let metrics = RecordingMetrics::default();
    let pool = InstrumentedPool::new(ctx.db.clone(), metrics.clone());

    let mut conn = pool.get().await?;
    conn.batch_execute(
        "CREATE TABLE widget (id int primary key);
         INSERT INTO widget (id) VALUES (1);",
    )
    .await?;

    let txn = conn.begin_instrumented().await?;
    txn.batch_execute("INSERT INTO widget (id) VALUES (2); INSERT INTO widget (id) VALUES (3);")
        .await?;
    txn.commit().await?;

    assert_eq!(conn.query_all("SELECT id FROM widget", &[]).await?.len(), 3);

    assert_eq!(
        metrics.count("query_success:batch_execute:None"),
        2,
        "batch_execute reports no row count through either wrapper: {:?}",
        metrics.events()
    );
    assert_eq!(metrics.count("query_error"), 0);

    let error = conn
        .batch_execute("CREATE TABLE widget (id int primary key);")
        .await
        .expect_err("the table already exists");
    assert!(matches!(error, DbErr::Postgres(_)));
    assert_eq!(metrics.count("query_error:batch_execute"), 1);

    drop(conn);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:metric.layer.tx+1/test]    dropping an instrumented transaction records nothing
#[pgorm_macros::test]
pub async fn dropped_instrumented_transaction_records_nothing() -> Result<(), DbErr> {
    let ctx = TestContext::new("metric_layer_drop_metrictx").await;
    let metrics = RecordingMetrics::default();
    let pool = InstrumentedPool::new(ctx.db.clone(), metrics.clone());

    let mut conn = pool.get().await?;
    conn.execute("CREATE TABLE widget (id int primary key)", &[])
        .await?;

    {
        let txn = conn.begin_instrumented().await?;
        txn.execute("INSERT INTO widget (id) VALUES (1)", &[])
            .await?;
    }

    assert_eq!(metrics.count("transaction_rollback"), 0);
    assert_eq!(metrics.count("transaction_commit"), 0);
    assert_eq!(conn.query_all("SELECT id FROM widget", &[]).await?.len(), 0);

    drop(conn);
    ctx.delete().await;

    Ok(())
}

/// Collects `(level, message)` for every `tracing` event emitted while installed.
#[derive(Clone, Default)]
struct CapturedEvents(Arc<Mutex<Vec<(Level, String)>>>);

impl CapturedEvents {
    fn taken(&self) -> Vec<(Level, String)> {
        std::mem::take(&mut self.0.lock().unwrap())
    }
}

#[derive(Default)]
struct MessageVisitor(String);

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

impl<S> Layer<S> for CapturedEvents
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        self.0
            .lock()
            .unwrap()
            .push((*event.metadata().level(), visitor.0));
    }
}

/// Drives all seven hooks of a collector, in declaration order.
async fn drive_all_hooks<M: MetricsCollector>(metrics: &M) {
    let elapsed = Duration::from_millis(1);
    let error = DbErr::Custom("boom".to_owned());

    metrics
        .record_query_success("execute", elapsed, Some(3))
        .await;
    metrics.record_query_error("execute", elapsed, &error).await;
    metrics.record_connection_acquired(elapsed).await;
    metrics.record_connection_error(elapsed, &error).await;
    metrics.record_transaction_begin(elapsed).await;
    metrics.record_transaction_commit(elapsed).await;
    metrics.record_transaction_rollback(elapsed).await;
}

// [spec:pgorm:def:metric.layer.collector/test]    seven hook points, all required of an implementor
#[pgorm_macros::test]
pub async fn collector_defines_seven_async_hooks() {
    let metrics = RecordingMetrics::default();
    drive_all_hooks(&metrics).await;

    assert_eq!(
        metrics.events(),
        vec![
            "query_success:execute:Some(3)".to_owned(),
            "query_error:execute".to_owned(),
            "connection_acquired".to_owned(),
            "connection_error".to_owned(),
            "transaction_begin".to_owned(),
            "transaction_commit".to_owned(),
            "transaction_rollback".to_owned(),
        ],
        "every hook dispatches to the implementor"
    );
}

// [spec:pgorm:def:metric.layer.collector/test]    NoOpMetrics observes nothing at all
#[pgorm_macros::test]
pub async fn noop_metrics_hooks_do_nothing() {
    let captured = CapturedEvents::default();
    let guard = tracing_subscriber::registry()
        .with(captured.clone())
        .set_default();

    drive_all_hooks(&NoOpMetrics).await;
    let events = captured.taken();

    drop(guard);
    assert_eq!(
        events,
        Vec::new(),
        "the no-op collector's hooks have empty bodies"
    );
}

// [spec:pgorm:def:metric.layer.collector/test]    LoggingMetrics' tracing levels per hook
#[pgorm_macros::test]
pub async fn logging_metrics_emits_expected_levels() {
    let captured = CapturedEvents::default();
    let guard = tracing_subscriber::registry()
        .with(captured.clone())
        .set_default();

    drive_all_hooks(&LoggingMetrics).await;
    // A success with no row count takes the other branch of the same hook.
    LoggingMetrics
        .record_query_success("query_raw", Duration::from_millis(1), None)
        .await;
    let events = captured.taken();

    drop(guard);

    let levels: Vec<Level> = events.iter().map(|(level, _)| *level).collect();
    assert_eq!(
        levels,
        vec![
            Level::DEBUG, // query success
            Level::WARN,  // query error
            Level::DEBUG, // connection acquired
            Level::ERROR, // connection error
            Level::DEBUG, // transaction begin
            Level::DEBUG, // transaction commit
            Level::WARN,  // transaction rollback
            Level::DEBUG, // query success, no row count
        ],
        "{events:?}"
    );
    assert!(events[0].1.contains("3 rows"), "{:?}", events[0]);
    assert!(events[1].1.contains("execute"), "{:?}", events[1]);
    assert!(!events[7].1.contains("rows"), "{:?}", events[7]);
}

// [spec:pgorm:def:metric.layer/test]    three wrappers, their inner()/metrics() accessors, and the pool's tag()/status()
#[pgorm_macros::test]
pub async fn wrappers_expose_inner_and_metrics() -> Result<(), DbErr> {
    fn assert_collector_bounds<M: MetricsCollector>() {
        fn assert_bounds<T: Clone + Send + Sync + 'static>() {}
        assert_bounds::<M>();
    }
    assert_collector_bounds::<RecordingMetrics>();
    assert_collector_bounds::<NoOpMetrics>();
    assert_collector_bounds::<LoggingMetrics>();

    let ctx = TestContext::new("metric_layer_accessors_metricacc").await;
    let metrics = RecordingMetrics::default();
    let pool: InstrumentedPool<RecordingMetrics> =
        InstrumentedPool::new(ctx.db.clone(), metrics.clone());

    // The pool wrapper forwards tag() and status(), and hands back both the wrapped
    // pool and the collector.
    assert_eq!(pool.tag(), ctx.db.tag());
    assert_eq!(pool.status().max_size, pool.inner().status().max_size);
    assert_eq!(pool.inner().status().size, ctx.db.status().size);
    metrics.push("probe".to_owned());
    assert_eq!(pool.metrics().events(), metrics.events());

    let mut conn: InstrumentedConnection<RecordingMetrics> = pool.get().await?;
    assert_eq!(metrics.count("connection_acquired"), 1);
    assert_eq!(conn.metrics().events(), metrics.events());

    // inner() reaches the wrapped connection, which carries no metrics hooks.
    let before = metrics.events().len();
    conn.inner()
        .execute("CREATE TABLE widget (id int primary key)", &[])
        .await?;
    assert_eq!(
        metrics.events().len(),
        before,
        "the unwrapped connection records nothing: {:?}",
        metrics.events()
    );
    conn.execute("INSERT INTO widget (id) VALUES (1)", &[])
        .await?;
    assert_eq!(metrics.count("query_success:execute"), 1);

    let txn: InstrumentedTransaction<'_, RecordingMetrics> = conn.begin_instrumented().await?;
    assert_eq!(txn.metrics().events(), metrics.events());
    let inner_txn = txn.inner().expect("the transaction has not been consumed");
    let before = metrics.events().len();
    inner_txn
        .execute("INSERT INTO widget (id) VALUES (2)", &[])
        .await?;
    assert_eq!(
        metrics.events().len(),
        before,
        "the unwrapped transaction records nothing: {:?}",
        metrics.events()
    );
    txn.commit().await?;

    assert_eq!(conn.query_all("SELECT id FROM widget", &[]).await?.len(), 2);

    drop(conn);
    ctx.delete().await;

    Ok(())
}
