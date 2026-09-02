#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, setup::*};

use async_trait::async_trait;
use pgorm::{
    ConnectionTrait, DbErr,
    metric::{InstrumentedPool, MetricsCollector},
};
use pretty_assertions::assert_eq;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
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
