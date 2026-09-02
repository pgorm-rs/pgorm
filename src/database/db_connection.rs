use std::{collections::BTreeMap, sync::Arc};

use crate::{ConnectionTrait, RetryableError, SqlText, TransactionTrait, error::*};
use deadpool::Status;
use pgorm_pool::{Object, Pool, Transaction};
use tokio_postgres::{
    IsolationLevel, RowStream, ToStatement,
    types::{BorrowToSql, ToSql},
};

/// Handle a database connection depending on the backend enabled by the feature
/// flags. This creates a database pool.
// [spec:pgorm:req:conn.pool.no-conn-trait]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct DatabasePool(pub(crate) Pool);

/// A set of [`DatabasePool`]s keyed by their tags, as built by
/// [`connect_multi_with_builder`](crate::connect_multi_with_builder).
// [spec:pgorm:sem:conn.pool.multi+1]    tag-keyed accessor surface
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct DatabaseMultiPool(pub(crate) BTreeMap<Arc<String>, DatabasePool>);

impl DatabaseMultiPool {
    /// The pool tagged `key`, or `None` if no pool carries that tag.
    pub fn get(&self, key: Arc<String>) -> Option<DatabasePool> {
        self.0.get(&key).cloned()
    }

    /// The current [`Status`] of every pool, keyed by tag.
    pub fn status(&self) -> BTreeMap<Arc<String>, Status> {
        self.0
            .iter()
            .map(|(k, v)| (k.clone(), v.status()))
            .collect()
    }
}

// [spec:pgorm:sem:conn.pool.get]
impl DatabasePool {
    /// Take a connection from the pool, waiting for one to become available.
    pub async fn get(&self) -> Result<DatabaseConnection, DbErr> {
        let conn = Pool::get(&self.0).await?;
        Ok(DatabaseConnection(conn))
    }

    /// The tag this pool was built with, or its generated `default-N` tag.
    pub fn tag(&self) -> Arc<String> {
        self.0.manager().tag()
    }

    /// A snapshot of the pool's size and how many connections are in use.
    pub fn status(&self) -> Status {
        self.0.status()
    }
}

/// A connection checked out of a [`DatabasePool`], returned to the pool on drop.
#[derive(Debug)]
pub struct DatabaseConnection(pub(crate) Object);

/// How [`DatabaseConnection::begin_with`] opens a transaction: the isolation
/// level and access mode it selects, as the combinations PostgreSQL acts on.
///
/// `DEFERRABLE` is reachable only through [`TransactionMode::DeferrableSnapshot`],
/// which carries the whole `SERIALIZABLE READ ONLY DEFERRABLE` combination,
/// because that is the only one in which the server honours it.
// [spec:pgorm:req:conn.tx+2]    transaction configuration
#[derive(Debug, Clone, Copy, Default)]
pub enum TransactionMode {
    /// Inherit the session defaults: `START TRANSACTION` with no clause, the
    /// same transaction [`TransactionTrait::begin`] opens.
    #[default]
    Default,
    /// `READ WRITE`, emitted explicitly so it overrides a session running
    /// under `SET default_transaction_read_only = on`.
    ReadWrite {
        /// `ISOLATION LEVEL <level>`; `None` inherits the session default.
        isolation: Option<IsolationLevel>,
    },
    /// `READ ONLY`: the server rejects any write with SQLSTATE `25006`.
    ReadOnly {
        /// `ISOLATION LEVEL <level>`; `None` inherits the session default.
        isolation: Option<IsolationLevel>,
    },
    /// `ISOLATION LEVEL SERIALIZABLE, READ ONLY, DEFERRABLE`: opening it may
    /// block until a snapshot is safe, after which the transaction cannot fail
    /// with a serialization error.
    DeferrableSnapshot,
}

impl DatabaseConnection {
    /// Open a transaction configured by `mode`.
    ///
    /// This is an inherent method rather than a [`TransactionTrait`] one
    /// because a nested transaction is a savepoint, and `SAVEPOINT` takes
    /// neither an isolation level nor an access mode.
    // [spec:pgorm:req:conn.tx+2]    configured START TRANSACTION on the pooled client
    pub async fn begin_with(
        &mut self,
        mode: TransactionMode,
    ) -> Result<DatabaseTransaction<'_>, DbErr> {
        let mut builder = self.0.build_transaction();

        match mode {
            TransactionMode::Default => {}
            TransactionMode::ReadWrite { isolation } => {
                if let Some(level) = isolation {
                    builder = builder.isolation_level(level);
                }
                builder = builder.read_only(false);
            }
            TransactionMode::ReadOnly { isolation } => {
                if let Some(level) = isolation {
                    builder = builder.isolation_level(level);
                }
                builder = builder.read_only(true);
            }
            TransactionMode::DeferrableSnapshot => {
                builder = builder
                    .isolation_level(IsolationLevel::Serializable)
                    .read_only(true)
                    .deferrable(true);
            }
        }

        Ok(DatabaseTransaction(Some(builder.start().await?)))
    }

    /// Run `f` inside a transaction, committing when it returns `Ok` and
    /// rolling back when it returns `Err`.
    ///
    /// The rollback is awaited, so the transaction is over by the time this
    /// returns. `Ok` is returned only once `COMMIT` succeeded; a failing
    /// `BEGIN` or `COMMIT` is [`TransactionError::Connection`] and the
    /// closure's own error is [`TransactionError::Transaction`].
    // [spec:pgorm:sem:conn.tx.closure]    plain BEGIN
    pub async fn transaction<'s, T, E, F>(&'s mut self, f: F) -> Result<T, TransactionError<E>>
    where
        F: AsyncFnOnce(&mut DatabaseTransaction<'s>) -> Result<T, E>,
    {
        let txn = TransactionTrait::begin(self)
            .await
            .map_err(TransactionError::Connection)?;

        drive(txn, f).await
    }

    /// [`DatabaseConnection::transaction`] over a transaction configured by
    /// `mode`, as [`DatabaseConnection::begin_with`] would open it.
    // [spec:pgorm:sem:conn.tx.closure]    configured BEGIN
    pub async fn transaction_with<'s, T, E, F>(
        &'s mut self,
        mode: TransactionMode,
        f: F,
    ) -> Result<T, TransactionError<E>>
    where
        F: AsyncFnOnce(&mut DatabaseTransaction<'s>) -> Result<T, E>,
    {
        let txn = DatabaseConnection::begin_with(self, mode)
            .await
            .map_err(TransactionError::Connection)?;

        drive(txn, f).await
    }

    /// Run `f` inside a transaction configured by `mode`, retrying the whole
    /// begin/run/commit cycle up to `max_retries` extra times while the failure
    /// is retryable.
    ///
    /// Intended for `IsolationLevel::Serializable`, where the server may reject
    /// a transaction that would break serial order and expects the client to
    /// replay it. `f` is [`AsyncFnMut`] because it is called once per attempt,
    /// and must therefore be replayable: every effect it has outside the
    /// transaction happens again on each attempt.
    // [spec:pgorm:sem:conn.tx.retry]
    pub async fn transaction_with_retry<T, E, F>(
        &mut self,
        mode: TransactionMode,
        max_retries: u32,
        mut f: F,
    ) -> Result<T, TransactionError<E>>
    where
        F: AsyncFnMut(&mut DatabaseTransaction<'_>) -> Result<T, E>,
        E: RetryableError,
    {
        let mut retries = 0;

        loop {
            let mut txn = self
                .begin_with(mode)
                .await
                .map_err(TransactionError::Connection)?;

            let outcome = match f(&mut txn).await {
                Ok(value) => match txn.commit().await {
                    Ok(()) => return Ok(value),
                    Err(error) => Retryable::Connection(error),
                },
                Err(error) => {
                    rollback(txn).await;
                    Retryable::Transaction(error)
                }
            };

            let (retryable, raised_by) = match &outcome {
                Retryable::Connection(error) => (error.is_retryable(), "commit"),
                Retryable::Transaction(error) => (error.is_retryable(), "closure"),
            };

            if !retryable || retries == max_retries {
                return Err(match outcome {
                    Retryable::Connection(error) => TransactionError::Connection(error),
                    Retryable::Transaction(error) => TransactionError::Transaction(error),
                });
            }

            retries += 1;
            tracing::debug!(retries, max_retries, raised_by, "retrying transaction");
        }
    }
}

enum Retryable<E> {
    Connection(DbErr),
    Transaction(E),
}

// [spec:pgorm:sem:conn.tx.closure]    commit on Ok, awaited rollback on Err
async fn drive<'s, T, E, F>(
    mut txn: DatabaseTransaction<'s>,
    f: F,
) -> Result<T, TransactionError<E>>
where
    F: AsyncFnOnce(&mut DatabaseTransaction<'s>) -> Result<T, E>,
{
    match f(&mut txn).await {
        Ok(value) => txn
            .commit()
            .await
            .map(|()| value)
            .map_err(TransactionError::Connection),
        Err(error) => {
            rollback(txn).await;
            Err(TransactionError::Transaction(error))
        }
    }
}

// [spec:pgorm:sem:conn.tx.closure]    a failed ROLLBACK does not displace the closure error
async fn rollback(txn: DatabaseTransaction<'_>) {
    if let Err(error) = txn.rollback().await {
        tracing::error!(
            %error,
            "ROLLBACK failed after the transaction closure returned an error"
        );
    }
}

/// Why a closure transaction did not commit.
///
/// The two variants keep the closure's own failure distinct from a failure of
/// the transaction machinery itself, which is why there is deliberately no
/// `From<DbErr>` impl: with `E = DbErr` it would silently file every
/// closure-side error under `Connection` and erase that distinction.
// [spec:pgorm:sem:conn.tx.closure]    error wrapper
#[derive(Debug)]
pub enum TransactionError<E> {
    /// `BEGIN`, `COMMIT`, or acquiring the transaction failed.
    Connection(DbErr),
    /// The closure returned an error; the transaction was rolled back.
    Transaction(E),
}

impl<E> std::fmt::Display for TransactionError<E>
where
    E: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connection(error) => write!(f, "Transaction Connection Error: {error}"),
            Self::Transaction(error) => write!(f, "Transaction Error: {error}"),
        }
    }
}

impl<E> std::error::Error for TransactionError<E>
where
    E: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Connection(error) => Some(error),
            Self::Transaction(error) => Some(error),
        }
    }
}

// [spec:pgorm:sem:conn.tx.guard+1]
/// An open transaction, borrowing the [`DatabaseConnection`] or parent
/// transaction it was opened on. Rolls back if dropped uncommitted.
#[derive(Debug)]
pub struct DatabaseTransaction<'a>(pub(crate) Option<Transaction<'a>>);

impl<'a> DatabaseTransaction<'a> {
    /// The open transaction. The `Option` is emptied only by `commit` and
    /// `rollback`, both of which consume the handle, so it is always `Some`
    /// here.
    fn tx(&self) -> &Transaction<'a> {
        self.0.as_ref().expect("transaction already consumed")
    }

    /// As [`DatabaseTransaction::tx`], for the mutable borrow a nested
    /// savepoint needs.
    fn tx_mut(&mut self) -> &mut Transaction<'a> {
        self.0.as_mut().expect("transaction already consumed")
    }

    /// Commits the transaction, making every change made within it permanent.
    pub async fn commit(mut self) -> Result<(), DbErr> {
        if let Some(tx) = self.0.take() {
            tx.commit().await.map_err(DbErr::Postgres)
        } else {
            unreachable!()
        }
    }

    /// Rolls the transaction back, discarding every change made within it.
    ///
    /// Dropping an uncommitted handle also rolls back, but the two paths differ:
    /// `Drop` queues a `ROLLBACK` on the connection and returns immediately,
    /// discarding the server's response, so a failure is invisible and only a
    /// `tracing::warn!` marks that it happened. This method awaits the
    /// `ROLLBACK` round trip and surfaces any failure as `DbErr::Postgres`, and
    /// consumes the handle so no warning is emitted.
    pub async fn rollback(mut self) -> Result<(), DbErr> {
        if let Some(tx) = self.0.take() {
            tx.rollback().await.map_err(DbErr::Postgres)
        } else {
            unreachable!()
        }
    }
}

// [spec:pgorm:sem:conn.tx.guard+1]    rollback-on-drop warning
impl Drop for DatabaseTransaction<'_> {
    fn drop(&mut self) {
        if self.0.is_some() {
            tracing::warn!("Transaction dropped without committing!");
        }
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for &DatabaseConnection {
    // #[instrument(level = "trace")]
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        Ok(self.0.execute(statement, params).await?)
    }

    // #[instrument(level = "trace")]
    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        Ok(self.0.execute_raw(statement, params).await?)
    }

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<tokio_postgres::Row, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        Ok(self.0.query_one(statement, params).await?)
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        Ok(self.0.query_opt(statement, params).await?)
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        Ok(self.0.query(statement, params).await?)
    }

    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        Ok(self.0.query_raw(statement, params).await?)
    }

    async fn batch_execute(&self, sql: &str) -> Result<(), DbErr> {
        Ok(pgorm_pool::GenericClient::batch_execute(&self.0, sql).await?)
    }
}

// [spec:pgorm:def:conn.pool.conn-trait+3]    delegating impls
#[async_trait::async_trait]
impl ConnectionTrait for DatabaseConnection {
    // #[instrument(level = "trace")]
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        Ok(self.0.execute(statement, params).await?)
    }

    // #[instrument(level = "trace")]
    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        Ok(self.0.execute_raw(statement, params).await?)
    }

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<tokio_postgres::Row, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        Ok(self.0.query_one(statement, params).await?)
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        Ok(self.0.query_opt(statement, params).await?)
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        Ok(self.0.query(statement, params).await?)
    }

    // [spec:pgorm:def:exec.stream]    pooled-client row stream
    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        Ok(self.0.query_raw(statement, params).await?)
    }

    async fn batch_execute(&self, sql: &str) -> Result<(), DbErr> {
        Ok(pgorm_pool::GenericClient::batch_execute(&self.0, sql).await?)
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for DatabaseTransaction<'_> {
    // #[instrument(level = "trace")]
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        Ok(self.tx().execute(statement, params).await?)
    }

    // #[instrument(level = "trace")]
    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        Ok(self.tx().execute_raw(statement, params).await?)
    }

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<tokio_postgres::Row, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        Ok(self.tx().query_one(statement, params).await?)
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        Ok(self.tx().query_opt(statement, params).await?)
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        Ok(self.tx().query(statement, params).await?)
    }

    // [spec:pgorm:def:exec.stream]    in-transaction row stream
    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, DbErr>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        Ok(self.tx().query_raw(statement, params).await?)
    }

    async fn batch_execute(&self, sql: &str) -> Result<(), DbErr> {
        Ok(pgorm_pool::GenericClient::batch_execute(self.tx(), sql).await?)
    }
}
#[async_trait::async_trait]
impl TransactionTrait for DatabaseTransaction<'_> {
    async fn begin(&mut self) -> Result<DatabaseTransaction<'_>, DbErr> {
        Ok(DatabaseTransaction(Some(
            self.tx_mut().transaction().await?,
        )))
    }
}

// [spec:pgorm:req:conn.tx+2]    BEGIN on the pooled client
#[async_trait::async_trait]
impl TransactionTrait for DatabaseConnection {
    async fn begin(&mut self) -> Result<DatabaseTransaction<'_>, DbErr> {
        let tx = self.0.transaction().await?;
        Ok(DatabaseTransaction(Some(tx)))
    }
}
#[cfg(test)]
mod tests {
    use crate::DatabasePool;

    #[test]
    fn assert_database_connection_traits() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<DatabasePool>();
    }
}
