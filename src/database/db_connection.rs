use std::{collections::BTreeMap, sync::Arc};

use crate::{ConnectionTrait, RetryableError, TransactionTrait, error::*};
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

// [spec:pgorm:sem:conn.pool.multi]    tag-keyed accessor surface
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct DatabaseMultiPool(pub(crate) BTreeMap<Arc<String>, DatabasePool>);

impl DatabaseMultiPool {
    pub fn get(&self, key: Arc<String>) -> Option<DatabasePool> {
        self.0.get(&key).cloned()
    }

    pub fn status(&self) -> BTreeMap<Arc<String>, Status> {
        self.0
            .iter()
            .map(|(k, v)| (k.clone(), v.status()))
            .collect()
    }
}

// impl Deref for DatabasePool {
//     type Target = Pool;

//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }

// [spec:pgorm:sem:conn.pool.get]
impl DatabasePool {
    pub async fn get(&self) -> Result<DatabaseConnection, DbErr> {
        let conn = Pool::get(&self.0).await?;
        Ok(DatabaseConnection(conn))
    }

    pub fn tag(&self) -> Arc<String> {
        self.0.manager().tag()
    }

    pub fn status(&self) -> Status {
        self.0.status()
    }
}

#[derive(Debug)]
pub struct DatabaseConnection(pub(crate) Object);

/// Isolation level, access mode, and deferrability for a transaction opened
/// with [`DatabaseConnection::begin_with`]. The default leaves every option
/// unset, which is equivalent to [`TransactionTrait::begin`].
// [spec:pgorm:req:conn.tx+1]    transaction configuration
#[derive(Debug, Clone, Copy, Default)]
pub struct TransactionOptions {
    /// Isolation level; `None` inherits the session default.
    pub isolation_level: Option<IsolationLevel>,
    /// Open the transaction `READ ONLY`.
    pub read_only: bool,
    /// Open the transaction `DEFERRABLE`.
    pub deferrable: bool,
}

impl DatabaseConnection {
    /// Execute SQL `BEGIN` with the given isolation level, access mode, and
    /// deferrability.
    ///
    /// This is an inherent method rather than a [`TransactionTrait`] one
    /// because a nested transaction is a savepoint, and `SAVEPOINT` takes none
    /// of these options.
    // [spec:pgorm:req:conn.tx+1]    configured BEGIN on the pooled client
    pub async fn begin_with(
        &mut self,
        opts: TransactionOptions,
    ) -> Result<DatabaseTransaction<'_>, DbErr> {
        let mut builder = self.0.build_transaction();

        if let Some(level) = opts.isolation_level {
            builder = builder.isolation_level(level);
        }

        if opts.read_only {
            builder = builder.read_only(true);
        }

        if opts.deferrable {
            builder = builder.deferrable(true);
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
    /// `opts`, as [`DatabaseConnection::begin_with`] would open it.
    // [spec:pgorm:sem:conn.tx.closure]    configured BEGIN
    pub async fn transaction_with<'s, T, E, F>(
        &'s mut self,
        opts: TransactionOptions,
        f: F,
    ) -> Result<T, TransactionError<E>>
    where
        F: AsyncFnOnce(&mut DatabaseTransaction<'s>) -> Result<T, E>,
    {
        let txn = DatabaseConnection::begin_with(self, opts)
            .await
            .map_err(TransactionError::Connection)?;

        drive(txn, f).await
    }

    /// Run `f` inside a transaction configured by `opts`, retrying the whole
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
        opts: TransactionOptions,
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
                .begin_with(opts)
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
#[derive(Debug)]
pub struct DatabaseTransaction<'a>(pub(crate) Option<Transaction<'a>>);

impl DatabaseTransaction<'_> {
    pub async fn commit(mut self) -> Result<(), DbErr> {
        if let Some(tx) = self.0.take() {
            tx.commit().await.map_err(|e| DbErr::Postgres(e))
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
            tx.rollback().await.map_err(|e| DbErr::Postgres(e))
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
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.execute(statement, params).await?)
    }

    // #[instrument(level = "trace")]
    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
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
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.query_one(statement, params).await?)
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.query_opt(statement, params).await?)
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.query(statement, params).await?)
    }

    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
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

// [spec:pgorm:def:conn.pool.conn-trait+2]    delegating impls
#[async_trait::async_trait]
impl ConnectionTrait for DatabaseConnection {
    // #[instrument(level = "trace")]
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.execute(statement, params).await?)
    }

    // #[instrument(level = "trace")]
    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
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
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.query_one(statement, params).await?)
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.query_opt(statement, params).await?)
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.query(statement, params).await?)
    }

    // [spec:pgorm:def:exec.stream]    pooled-client row stream
    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
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
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.as_ref().unwrap().execute(statement, params).await?)
    }

    // #[instrument(level = "trace")]
    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        Ok(self
            .0
            .as_ref()
            .unwrap()
            .execute_raw(statement, params)
            .await?)
    }

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<tokio_postgres::Row, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self
            .0
            .as_ref()
            .unwrap()
            .query_one(statement, params)
            .await?)
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self
            .0
            .as_ref()
            .unwrap()
            .query_opt(statement, params)
            .await?)
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.as_ref().unwrap().query(statement, params).await?)
    }

    // [spec:pgorm:def:exec.stream]    in-transaction row stream
    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        Ok(self
            .0
            .as_ref()
            .unwrap()
            .query_raw(statement, params)
            .await?)
    }

    async fn batch_execute(&self, sql: &str) -> Result<(), DbErr> {
        Ok(pgorm_pool::GenericClient::batch_execute(self.0.as_ref().unwrap(), sql).await?)
    }
}
#[async_trait::async_trait]
impl TransactionTrait for DatabaseTransaction<'_> {
    async fn begin(&mut self) -> Result<DatabaseTransaction<'_>, DbErr> {
        Ok(DatabaseTransaction(Some(
            self.0.as_mut().unwrap().transaction().await?,
        )))
    }
}

// [spec:pgorm:req:conn.tx+1]    BEGIN on the pooled client
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
