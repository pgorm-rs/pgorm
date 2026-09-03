mod fingerprint;

pub use fingerprint::{QueryContext, QueryFingerprint};

use crate::{
    ConnectionTrait, DatabaseConnection, DatabasePool, DatabaseTransaction, Error, SqlText,
    TransactionTrait,
};
use async_trait::async_trait;
use fingerprint::FingerprintSuffix;
use std::time::{Duration, Instant};
use tokio_postgres::{
    Row, RowStream,
    types::{BorrowToSql, ToSql},
};

/// The body every instrumented `ConnectionTrait` method shares: build the
/// context, time the awaited delegation, report the outcome under the hook it
/// selects, and hand back the wrapped value's own result untouched.
///
/// What actually differs between the fourteen methods is three things — the
/// operation name, where the SQL text comes from, and how a row count is read
/// out of the `Ok` payload — so those are the parameters and the rest is
/// written once. `$rows` is evaluated against the payload by reference.
// [spec:pgorm:req:metric.layer.delegate+4]    the timing and reporting body
macro_rules! report {
    ($self:ident, $operation:literal, $sql:expr, $call:expr, $ok:pat => $rows:expr) => {{
        let query = QueryContext::new($operation, $sql);
        let start = Instant::now();
        let result = $call;
        let elapsed = start.elapsed();

        match &result {
            Ok($ok) => {
                $self
                    .metrics
                    .record_query_success(query, elapsed, $rows)
                    .await;
            }
            Err(error) => {
                $self
                    .metrics
                    .record_query_error(query, elapsed, error)
                    .await;
            }
        }

        result
    }};
}

/// The transaction behind an [`InstrumentedTransaction`] that has not been
/// consumed.
///
/// `commit` and `rollback` take it by value and leave `None` behind; both
/// consume the handle, so a statement method finding `None` is a bug in this
/// module rather than anything a caller can provoke.
macro_rules! open_transaction {
    ($self:ident) => {
        match &$self.transaction {
            Some(transaction) => transaction,
            None => unreachable!("Transaction already consumed"),
        }
    };
}

/// Trait for collecting database metrics
// [spec:pgorm:def:metric.layer.collector+1]
#[async_trait]
pub trait MetricsCollector: Clone + Send + Sync + 'static {
    /// Record a successful database operation
    async fn record_query_success(
        &self,
        query: QueryContext<'_>,
        duration: Duration,
        rows: Option<u64>,
    );

    /// Record a failed database operation
    async fn record_query_error(&self, query: QueryContext<'_>, duration: Duration, error: &Error);

    /// Record connection acquisition
    async fn record_connection_acquired(&self, duration: Duration);

    /// Record connection acquisition failure
    async fn record_connection_error(&self, duration: Duration, error: &Error);

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
        _query: QueryContext<'_>,
        _duration: Duration,
        _rows: Option<u64>,
    ) {
    }
    async fn record_query_error(
        &self,
        _query: QueryContext<'_>,
        _duration: Duration,
        _error: &Error,
    ) {
    }
    async fn record_connection_acquired(&self, _duration: Duration) {}
    async fn record_connection_error(&self, _duration: Duration, _error: &Error) {}
    async fn record_transaction_begin(&self, _duration: Duration) {}
    async fn record_transaction_commit(&self, _duration: Duration) {}
    async fn record_transaction_rollback(&self, _duration: Duration) {}
}

/// A simple logging-based metrics collector
#[derive(Clone, Debug)]
pub struct LoggingMetrics;

// [spec:pgorm:def:metric.layer.collector+1]    LoggingMetrics tracing levels
#[async_trait]
impl MetricsCollector for LoggingMetrics {
    async fn record_query_success(
        &self,
        query: QueryContext<'_>,
        duration: Duration,
        rows: Option<u64>,
    ) {
        let fingerprint = FingerprintSuffix(query.fingerprint());
        if let Some(rows) = rows {
            tracing::debug!(
                "Query {} succeeded in {:?}, {} rows{}",
                query.operation(),
                duration,
                rows,
                fingerprint
            );
        } else {
            tracing::debug!(
                "Query {} succeeded in {:?}{}",
                query.operation(),
                duration,
                fingerprint
            );
        }
    }

    async fn record_query_error(&self, query: QueryContext<'_>, duration: Duration, error: &Error) {
        tracing::warn!(
            "Query {} failed after {:?}: {:?}{}",
            query.operation(),
            duration,
            error,
            FingerprintSuffix(query.fingerprint())
        );
    }

    async fn record_connection_acquired(&self, duration: Duration) {
        tracing::debug!("Connection acquired in {:?}", duration);
    }

    async fn record_connection_error(&self, duration: Duration, error: &Error) {
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
// [spec:pgorm:def:metric.layer+1]    pool wrapper
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
    pub async fn get(&self) -> Result<InstrumentedConnection<M>, Error> {
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
// [spec:pgorm:def:metric.layer+1]    connection wrapper
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

    /// Begin a transaction that is already wrapped for instrumentation.
    ///
    /// Reports the same hooks as [`TransactionTrait::begin`], and hands back an
    /// [`InstrumentedTransaction`] sharing a clone of the collector, so
    /// statements issued inside the transaction stay instrumented without the
    /// caller wrapping the handle by hand.
    // [spec:pgorm:sem:metric.layer.tx+2]    instrumented begin
    pub async fn begin_instrumented(&mut self) -> Result<InstrumentedTransaction<'_, M>, Error> {
        let metrics = self.metrics.clone();
        let query = QueryContext::new("begin", None);
        let start = Instant::now();
        let result = self.connection.begin().await;
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => {
                metrics.record_transaction_begin(elapsed).await;
            }
            Err(e) => {
                metrics.record_query_error(query, elapsed, e).await;
            }
        }

        result.map(|transaction| InstrumentedTransaction::new(transaction, metrics))
    }
}

// [spec:pgorm:req:metric.layer.delegate+4]
#[async_trait]
impl<M: MetricsCollector> ConnectionTrait for InstrumentedConnection<M> {
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error>
    where
        T: ?Sized + SqlText + Sync,
    {
        report!(self, "execute", Some(statement.sql_text()),
            self.connection.execute(statement, params).await, rows => Some(*rows))
    }

    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, Error>
    where
        T: ?Sized + SqlText + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        report!(self, "execute_raw", Some(statement.sql_text()),
            self.connection.execute_raw(statement, params).await, rows => Some(*rows))
    }

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Row, Error>
    where
        T: ?Sized + SqlText + Sync,
    {
        report!(self, "query_one", Some(statement.sql_text()),
            self.connection.query_one(statement, params).await, _ => Some(1))
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<Row>, Error>
    where
        T: ?Sized + SqlText + Sync,
    {
        report!(self, "query_opt", Some(statement.sql_text()),
            self.connection.query_opt(statement, params).await, row => Some(if row.is_some() { 1 } else { 0 }))
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<Row>, Error>
    where
        T: ?Sized + SqlText + Sync,
    {
        report!(self, "query_all", Some(statement.sql_text()),
            self.connection.query_all(statement, params).await, rows => Some(rows.len() as u64))
    }

    // [spec:pgorm:sem:exec.stream.decode+1]    row count unknown at stream creation
    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, Error>
    where
        T: ?Sized + SqlText + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        report!(self, "query_raw", Some(statement.sql_text()),
            self.connection.query_raw(statement, params).await, _ => None)
    }

    async fn batch_execute(&self, sql: &str) -> Result<(), Error> {
        report!(self, "batch_execute", Some(sql),
            self.connection.batch_execute(sql).await, () => None)
    }
}

/// A transaction wrapper that instruments transaction operations
// [spec:pgorm:sem:metric.layer.tx+2]    commit + rollback reporting, no-op drop
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
    pub async fn commit(mut self) -> Result<(), Error> {
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

    /// Roll the transaction back, reporting the rollback to the collector.
    ///
    /// Dropping the handle instead rolls back on the connection but records
    /// nothing; this is the only path that reports a rollback the caller asked
    /// for rather than one Postgres forced by a failed commit.
    pub async fn rollback(mut self) -> Result<(), Error> {
        let query = QueryContext::new("rollback", None);
        let start = Instant::now();
        let metrics = self.metrics.clone();

        if let Some(transaction) = self.transaction.take() {
            let result = transaction.rollback().await;
            let elapsed = start.elapsed();

            if let Err(e) = &result {
                metrics.record_query_error(query, elapsed, e).await;
            }
            metrics.record_transaction_rollback(elapsed).await;

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

// [spec:pgorm:req:metric.layer.delegate+4]    statements inside a transaction
#[async_trait]
impl<M: MetricsCollector> ConnectionTrait for InstrumentedTransaction<'_, M> {
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error>
    where
        T: ?Sized + SqlText + Sync,
    {
        report!(self, "execute", Some(statement.sql_text()),
            open_transaction!(self).execute(statement, params).await, rows => Some(*rows))
    }

    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, Error>
    where
        T: ?Sized + SqlText + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        report!(self, "execute_raw", Some(statement.sql_text()),
            open_transaction!(self).execute_raw(statement, params).await, rows => Some(*rows))
    }

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Row, Error>
    where
        T: ?Sized + SqlText + Sync,
    {
        report!(self, "query_one", Some(statement.sql_text()),
            open_transaction!(self).query_one(statement, params).await, _ => Some(1))
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<Row>, Error>
    where
        T: ?Sized + SqlText + Sync,
    {
        report!(self, "query_opt", Some(statement.sql_text()),
            open_transaction!(self).query_opt(statement, params).await, row => Some(if row.is_some() { 1 } else { 0 }))
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<Row>, Error>
    where
        T: ?Sized + SqlText + Sync,
    {
        report!(self, "query_all", Some(statement.sql_text()),
            open_transaction!(self).query_all(statement, params).await, rows => Some(rows.len() as u64))
    }

    // [spec:pgorm:sem:exec.stream.decode+1]    row count unknown at stream creation
    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, Error>
    where
        T: ?Sized + SqlText + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        report!(self, "query_raw", Some(statement.sql_text()),
            open_transaction!(self).query_raw(statement, params).await, _ => None)
    }

    async fn batch_execute(&self, sql: &str) -> Result<(), Error> {
        report!(self, "batch_execute", Some(sql),
            open_transaction!(self).batch_execute(sql).await, () => None)
    }
}

// [spec:pgorm:sem:metric.layer.tx+2]    timed begin, uninstrumented handle
#[async_trait]
impl<M: MetricsCollector> TransactionTrait for InstrumentedConnection<M> {
    async fn begin(&mut self) -> Result<DatabaseTransaction<'_>, Error> {
        let query = QueryContext::new("begin", None);
        let start = Instant::now();
        let result = self.connection.begin().await;
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => {
                self.metrics.record_transaction_begin(elapsed).await;
            }
            Err(e) => {
                self.metrics.record_query_error(query, elapsed, e).await;
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
