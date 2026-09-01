use crate::{
    ConnectionTrait, DatabaseConnection, DatabasePool, DatabaseTransaction, DbErr, TransactionTrait,
};
use async_trait::async_trait;
use std::time::{Duration, Instant};
use tokio_postgres::{
    Row, ToStatement,
    types::{BorrowToSql, ToSql},
};

/// Trait for collecting database metrics
// [spec:pgorm:def:metric.layer.collector]
#[async_trait]
pub trait MetricsCollector: Clone + Send + Sync + 'static {
    /// Record a successful database operation
    async fn record_query_success(&self, operation: &str, duration: Duration, rows: Option<u64>);

    /// Record a failed database operation
    async fn record_query_error(&self, operation: &str, duration: Duration, error: &DbErr);

    /// Record connection acquisition
    async fn record_connection_acquired(&self, duration: Duration);

    /// Record connection acquisition failure
    async fn record_connection_error(&self, duration: Duration, error: &DbErr);

    /// Record transaction begin
    async fn record_transaction_begin(&self, duration: Duration);

    /// Record transaction commit
    async fn record_transaction_commit(&self, duration: Duration);

    /// Record transaction rollback
    async fn record_transaction_rollback(&self, duration: Duration);
}

/// A no-op metrics collector that does nothing (zero cost)
#[derive(Clone, Debug)]
pub struct NoOpMetrics;

#[async_trait]
impl MetricsCollector for NoOpMetrics {
    async fn record_query_success(
        &self,
        _operation: &str,
        _duration: Duration,
        _rows: Option<u64>,
    ) {
    }
    async fn record_query_error(&self, _operation: &str, _duration: Duration, _error: &DbErr) {}
    async fn record_connection_acquired(&self, _duration: Duration) {}
    async fn record_connection_error(&self, _duration: Duration, _error: &DbErr) {}
    async fn record_transaction_begin(&self, _duration: Duration) {}
    async fn record_transaction_commit(&self, _duration: Duration) {}
    async fn record_transaction_rollback(&self, _duration: Duration) {}
}

/// A simple logging-based metrics collector
#[derive(Clone, Debug)]
pub struct LoggingMetrics;

// [spec:pgorm:def:metric.layer.collector]    LoggingMetrics tracing levels
#[async_trait]
impl MetricsCollector for LoggingMetrics {
    async fn record_query_success(&self, operation: &str, duration: Duration, rows: Option<u64>) {
        if let Some(rows) = rows {
            tracing::debug!(
                "Query {} succeeded in {:?}, {} rows",
                operation,
                duration,
                rows
            );
        } else {
            tracing::debug!("Query {} succeeded in {:?}", operation, duration);
        }
    }

    async fn record_query_error(&self, operation: &str, duration: Duration, error: &DbErr) {
        tracing::warn!(
            "Query {} failed after {:?}: {:?}",
            operation,
            duration,
            error
        );
    }

    async fn record_connection_acquired(&self, duration: Duration) {
        tracing::debug!("Connection acquired in {:?}", duration);
    }

    async fn record_connection_error(&self, duration: Duration, error: &DbErr) {
        tracing::error!("Connection failed after {:?}: {:?}", duration, error);
    }

    async fn record_transaction_begin(&self, duration: Duration) {
        tracing::debug!("Transaction began in {:?}", duration);
    }

    async fn record_transaction_commit(&self, duration: Duration) {
        tracing::debug!("Transaction committed in {:?}", duration);
    }

    async fn record_transaction_rollback(&self, duration: Duration) {
        tracing::warn!("Transaction rolled back in {:?}", duration);
    }
}

/// A connection pool wrapper that produces instrumented connections
// [spec:pgorm:def:metric.layer]    pool wrapper
#[derive(Debug, Clone)]
pub struct InstrumentedPool<M: MetricsCollector> {
    pool: DatabasePool,
    metrics: M,
}

impl<M: MetricsCollector> InstrumentedPool<M> {
    /// Create a new instrumented pool
    pub fn new(pool: DatabasePool, metrics: M) -> Self {
        Self { pool, metrics }
    }

    /// Get an instrumented connection from the pool
    pub async fn get(&self) -> Result<InstrumentedConnection<M>, DbErr> {
        let start = Instant::now();
        let result = self.pool.get().await;
        let elapsed = start.elapsed();

        match result {
            Ok(conn) => {
                self.metrics.record_connection_acquired(elapsed).await;
                Ok(InstrumentedConnection::new(conn, self.metrics.clone()))
            }
            Err(e) => {
                self.metrics.record_connection_error(elapsed, &e).await;
                Err(e)
            }
        }
    }

    /// Get the pool tag
    pub fn tag(&self) -> std::sync::Arc<String> {
        self.pool.tag()
    }

    /// Get the pool status
    pub fn status(&self) -> deadpool::Status {
        self.pool.status()
    }

    /// Get a reference to the underlying pool
    pub fn inner(&self) -> &DatabasePool {
        &self.pool
    }

    /// Get a reference to the metrics collector
    pub fn metrics(&self) -> &M {
        &self.metrics
    }
}

/// A connection wrapper that instruments all database operations
// [spec:pgorm:def:metric.layer]    connection wrapper
#[derive(Debug)]
pub struct InstrumentedConnection<M: MetricsCollector> {
    connection: DatabaseConnection,
    metrics: M,
}

impl<M: MetricsCollector> InstrumentedConnection<M> {
    /// Create a new instrumented connection
    pub fn new(connection: DatabaseConnection, metrics: M) -> Self {
        Self {
            connection,
            metrics,
        }
    }

    /// Get a reference to the underlying connection
    pub fn inner(&self) -> &DatabaseConnection {
        &self.connection
    }

    /// Get a reference to the metrics collector
    pub fn metrics(&self) -> &M {
        &self.metrics
    }
}

// [spec:pgorm:req:metric.layer.delegate]
#[async_trait]
impl<M: MetricsCollector> ConnectionTrait for InstrumentedConnection<M> {
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        let start = Instant::now();
        let result = self.connection.execute(statement, params).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(rows) => {
                self.metrics
                    .record_query_success("execute", elapsed, Some(*rows))
                    .await;
            }
            Err(e) => {
                self.metrics.record_query_error("execute", elapsed, e).await;
            }
        }

        result
    }

    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        let start = Instant::now();
        let result = self.connection.execute_raw(statement, params).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(rows) => {
                self.metrics
                    .record_query_success("execute_raw", elapsed, Some(*rows))
                    .await;
            }
            Err(e) => {
                self.metrics
                    .record_query_error("execute_raw", elapsed, e)
                    .await;
            }
        }

        result
    }

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Row, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        let start = Instant::now();
        let result = self.connection.query_one(statement, params).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => {
                self.metrics
                    .record_query_success("query_one", elapsed, Some(1))
                    .await;
            }
            Err(e) => {
                self.metrics
                    .record_query_error("query_one", elapsed, e)
                    .await;
            }
        }

        result
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        let start = Instant::now();
        let result = self.connection.query_opt(statement, params).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(Some(_)) => {
                self.metrics
                    .record_query_success("query_opt", elapsed, Some(1))
                    .await;
            }
            Ok(None) => {
                self.metrics
                    .record_query_success("query_opt", elapsed, Some(0))
                    .await;
            }
            Err(e) => {
                self.metrics
                    .record_query_error("query_opt", elapsed, e)
                    .await;
            }
        }

        result
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        let start = Instant::now();
        let result = self.connection.query_all(statement, params).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(rows) => {
                self.metrics
                    .record_query_success("query_all", elapsed, Some(rows.len() as u64))
                    .await;
            }
            Err(e) => {
                self.metrics
                    .record_query_error("query_all", elapsed, e)
                    .await;
            }
        }

        result
    }
}

/// A transaction wrapper that instruments transaction operations
// [spec:pgorm:sem:metric.layer.tx]    commit reporting + no-op drop
#[derive(Debug)]
pub struct InstrumentedTransaction<'a, M: MetricsCollector> {
    transaction: Option<DatabaseTransaction<'a>>,
    metrics: M,
}

impl<'a, M: MetricsCollector> InstrumentedTransaction<'a, M> {
    /// Create a new instrumented transaction
    pub fn new(transaction: DatabaseTransaction<'a>, metrics: M) -> Self {
        Self {
            transaction: Some(transaction),
            metrics,
        }
    }

    /// Commit the transaction
    pub async fn commit(mut self) -> Result<(), DbErr> {
        let start = Instant::now();
        let metrics = self.metrics.clone();

        if let Some(transaction) = self.transaction.take() {
            let result = transaction.commit().await;
            let elapsed = start.elapsed();

            match &result {
                Ok(_) => {
                    metrics.record_transaction_commit(elapsed).await;
                }
                Err(_) => {
                    metrics.record_transaction_rollback(elapsed).await;
                }
            }

            result
        } else {
            unreachable!("Transaction already consumed")
        }
    }

    /// Get a reference to the underlying transaction
    pub fn inner(&self) -> Option<&DatabaseTransaction<'a>> {
        self.transaction.as_ref()
    }

    /// Get a reference to the metrics collector
    pub fn metrics(&self) -> &M {
        &self.metrics
    }
}

// [spec:pgorm:req:metric.layer.delegate]    statements inside a transaction
#[async_trait]
impl<M: MetricsCollector> ConnectionTrait for InstrumentedTransaction<'_, M> {
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        let start = Instant::now();
        let result = if let Some(transaction) = &self.transaction {
            transaction.execute(statement, params).await
        } else {
            unreachable!("Transaction already consumed")
        };
        let elapsed = start.elapsed();

        match &result {
            Ok(rows) => {
                self.metrics
                    .record_query_success("execute", elapsed, Some(*rows))
                    .await;
            }
            Err(e) => {
                self.metrics.record_query_error("execute", elapsed, e).await;
            }
        }

        result
    }

    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        let start = Instant::now();
        let result = if let Some(transaction) = &self.transaction {
            transaction.execute_raw(statement, params).await
        } else {
            unreachable!("Transaction already consumed")
        };
        let elapsed = start.elapsed();

        match &result {
            Ok(rows) => {
                self.metrics
                    .record_query_success("execute_raw", elapsed, Some(*rows))
                    .await;
            }
            Err(e) => {
                self.metrics
                    .record_query_error("execute_raw", elapsed, e)
                    .await;
            }
        }

        result
    }

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Row, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        let start = Instant::now();
        let result = if let Some(transaction) = &self.transaction {
            transaction.query_one(statement, params).await
        } else {
            unreachable!("Transaction already consumed")
        };
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => {
                self.metrics
                    .record_query_success("query_one", elapsed, Some(1))
                    .await;
            }
            Err(e) => {
                self.metrics
                    .record_query_error("query_one", elapsed, e)
                    .await;
            }
        }

        result
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        let start = Instant::now();
        let result = if let Some(transaction) = &self.transaction {
            transaction.query_opt(statement, params).await
        } else {
            unreachable!("Transaction already consumed")
        };
        let elapsed = start.elapsed();

        match &result {
            Ok(Some(_)) => {
                self.metrics
                    .record_query_success("query_opt", elapsed, Some(1))
                    .await;
            }
            Ok(None) => {
                self.metrics
                    .record_query_success("query_opt", elapsed, Some(0))
                    .await;
            }
            Err(e) => {
                self.metrics
                    .record_query_error("query_opt", elapsed, e)
                    .await;
            }
        }

        result
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        let start = Instant::now();
        let result = if let Some(transaction) = &self.transaction {
            transaction.query_all(statement, params).await
        } else {
            unreachable!("Transaction already consumed")
        };
        let elapsed = start.elapsed();

        match &result {
            Ok(rows) => {
                self.metrics
                    .record_query_success("query_all", elapsed, Some(rows.len() as u64))
                    .await;
            }
            Err(e) => {
                self.metrics
                    .record_query_error("query_all", elapsed, e)
                    .await;
            }
        }

        result
    }
}

// [spec:pgorm:sem:metric.layer.tx]    timed begin
#[async_trait]
impl<M: MetricsCollector> TransactionTrait for InstrumentedConnection<M> {
    async fn begin(&mut self) -> Result<DatabaseTransaction<'_>, DbErr> {
        let start = Instant::now();
        let result = self.connection.begin().await;
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => {
                self.metrics.record_transaction_begin(elapsed).await;
            }
            Err(e) => {
                self.metrics.record_query_error("begin", elapsed, e).await;
            }
        }

        result
    }
}

impl<M: MetricsCollector> Drop for InstrumentedTransaction<'_, M> {
    fn drop(&mut self) {
        // The underlying DatabaseTransaction will handle the rollback warning if still present
        // If transaction is None, it was already committed
    }
}
