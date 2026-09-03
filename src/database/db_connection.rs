use std::{collections::BTreeMap, sync::Arc};

use crate::{ConnectionTrait, RetryableError, SqlText, TransactionTrait, error::*};
use deadpool::Status;
use pgorm_pool::{Object, Pool, Transaction};
use tokio_postgres::{
    IsolationLevel, RowStream, ToStatement,
    error::SqlState,
    types::{BorrowToSql, ToSql},
};

/// Run a statement against the connection's statement cache, re-preparing once
/// if PostgreSQL rejects the cached plan.
///
/// `$owner` is whatever holds the cache — a pooled client or a transaction on
/// one. A statement that still carries its SQL text (`conn.sql-text`) is
/// resolved through the cache, so the same text is parsed once per connection
/// rather than once per call; a prepared [`Statement`](tokio_postgres::Statement)
/// answers `None` and is passed through untouched, since there is no text to
/// key a cache entry on and it is already prepared.
///
/// `$call` is written once and expanded against both, which type-checks in each
/// arm because every method it can wrap is generic over `ToStatement`. It is
/// expanded a third time for the retry, so it must be replayable: only the
/// methods whose parameters are a reusable `&[&dyn ToSql]` slice can use this
/// macro.
// [spec:pgorm:req:conn.pool.statement-cache.invalidate]    evict, re-prepare, retry once
macro_rules! cached {
    ($owner:expr, $statement:expr, |$prepared:ident| $call:expr) => {{
        let owner = $owner;
        match $statement.sql_text() {
            None => {
                let $prepared = $statement;
                Ok($call?)
            }
            Some(sql) => {
                let cached = owner.prepare_cached(sql).await?;
                let outcome = {
                    let $prepared = &cached;
                    $call
                };

                match outcome {
                    Err(error) if is_stale_cached_plan(&error) => {
                        drop(owner.statement_cache.remove(sql, &[]));
                        let reprepared = owner.prepare_cached(sql).await?;
                        let $prepared = &reprepared;
                        Ok($call?)
                    }
                    outcome => Ok(outcome?),
                }
            }
        }
    }};
}

/// [`cached`] for the two methods whose parameters are an `IntoIterator`.
///
/// Those parameters are consumed by the first attempt and cannot be re-supplied
/// — the iterator is not `Clone`, and holding its items across the retry would
/// demand a `Send` bound the trait does not carry — so a rejected plan is
/// evicted, which makes the next call re-prepare, and the error is returned as
/// it stands.
// [spec:pgorm:req:conn.pool.statement-cache.invalidate]    evict without a retry
macro_rules! cached_once {
    ($owner:expr, $statement:expr, |$prepared:ident| $call:expr) => {{
        let owner = $owner;
        match $statement.sql_text() {
            None => {
                let $prepared = $statement;
                Ok($call?)
            }
            Some(sql) => {
                let cached = owner.prepare_cached(sql).await?;
                let outcome = {
                    let $prepared = &cached;
                    $call
                };

                if let Err(error) = &outcome
                    && is_stale_cached_plan(error)
                {
                    drop(owner.statement_cache.remove(sql, &[]));
                }

                Ok(outcome?)
            }
        }
    }};
}

/// Whether PostgreSQL refused to run a plan because the statement it was built
/// for no longer produces the result it was described as producing.
///
/// The server reports it as SQLSTATE `0A000`, which is also its generic
/// *feature not supported*. Nothing distinguishes the two but the message text,
/// which is localized, so a statement rejected on its own merits is retried
/// once as well; it fails identically the second time, at the cost of one round
/// trip on a call that was already failing.
// [spec:pgorm:req:conn.pool.statement-cache.invalidate]    the SQLSTATE that means "re-prepare"
fn is_stale_cached_plan(error: &tokio_postgres::Error) -> bool {
    error.code() == Some(&SqlState::FEATURE_NOT_SUPPORTED)
}

/// Handle a database connection depending on the backend enabled by the feature
/// flags. This creates a database pool.
// [spec:pgorm:req:conn.pool.no-conn-trait]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct DatabasePool(pub(crate) Pool);

/// A set of [`DatabasePool`]s keyed by their tags, as built by
/// [`connect_multi_with_builder`](crate::connect_multi_with_builder).
// [spec:pgorm:sem:conn.pool.multi+2]    tag-keyed accessor surface
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

// [spec:pgorm:sem:conn.pool.get+1]
impl DatabasePool {
    /// Take a connection from the pool, waiting for one to become available.
    pub async fn get(&self) -> Result<DatabaseConnection, Error> {
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
    ) -> Result<DatabaseTransaction<'_>, Error> {
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
    // [spec:pgorm:sem:conn.tx.closure+1]    plain BEGIN
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
    // [spec:pgorm:sem:conn.tx.closure+1]    configured BEGIN
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
    // [spec:pgorm:sem:conn.tx.retry+1]
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
    Connection(Error),
    Transaction(E),
}

// [spec:pgorm:sem:conn.tx.closure+1]    commit on Ok, awaited rollback on Err
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

// [spec:pgorm:sem:conn.tx.closure+1]    a failed ROLLBACK does not displace the closure error
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
/// `From<Error>` impl: with `E = Error` it would silently file every
/// closure-side error under `Connection` and erase that distinction.
// [spec:pgorm:sem:conn.tx.closure+1]    error wrapper
#[derive(Debug)]
pub enum TransactionError<E> {
    /// `BEGIN`, `COMMIT`, or acquiring the transaction failed.
    Connection(Error),
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

// [spec:pgorm:sem:conn.tx.guard+2]
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
    pub async fn commit(mut self) -> Result<(), Error> {
        if let Some(tx) = self.0.take() {
            tx.commit().await.map_err(Error::Postgres)
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
    /// `ROLLBACK` round trip and surfaces any failure as `Error::Postgres`, and
    /// consumes the handle so no warning is emitted.
    pub async fn rollback(mut self) -> Result<(), Error> {
        if let Some(tx) = self.0.take() {
            tx.rollback().await.map_err(Error::Postgres)
        } else {
            unreachable!()
        }
    }
}

// [spec:pgorm:sem:conn.tx.guard+2]    rollback-on-drop warning
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
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        cached!(&self.0, statement, |prepared| self
            .0
            .execute(prepared, params)
            .await)
    }

    // #[instrument(level = "trace")]
    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        cached_once!(&self.0, statement, |prepared| self
            .0
            .execute_raw(prepared, params)
            .await)
    }

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<tokio_postgres::Row, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        cached!(&self.0, statement, |prepared| self
            .0
            .query_one(prepared, params)
            .await)
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<tokio_postgres::Row>, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        cached!(&self.0, statement, |prepared| self
            .0
            .query_opt(prepared, params)
            .await)
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        cached!(&self.0, statement, |prepared| self
            .0
            .query(prepared, params)
            .await)
    }

    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        cached_once!(&self.0, statement, |prepared| self
            .0
            .query_raw(prepared, params)
            .await)
    }

    async fn batch_execute(&self, sql: &str) -> Result<(), Error> {
        Ok(pgorm_pool::GenericClient::batch_execute(&self.0, sql).await?)
    }
}

// [spec:pgorm:def:conn.pool.conn-trait+5]    cache-routing impls
#[async_trait::async_trait]
impl ConnectionTrait for DatabaseConnection {
    // #[instrument(level = "trace")]
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        cached!(&self.0, statement, |prepared| self
            .0
            .execute(prepared, params)
            .await)
    }

    // #[instrument(level = "trace")]
    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        cached_once!(&self.0, statement, |prepared| self
            .0
            .execute_raw(prepared, params)
            .await)
    }

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<tokio_postgres::Row, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        cached!(&self.0, statement, |prepared| self
            .0
            .query_one(prepared, params)
            .await)
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<tokio_postgres::Row>, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        cached!(&self.0, statement, |prepared| self
            .0
            .query_opt(prepared, params)
            .await)
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        cached!(&self.0, statement, |prepared| self
            .0
            .query(prepared, params)
            .await)
    }

    // [spec:pgorm:def:exec.stream+1]    pooled-client row stream
    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        cached_once!(&self.0, statement, |prepared| self
            .0
            .query_raw(prepared, params)
            .await)
    }

    async fn batch_execute(&self, sql: &str) -> Result<(), Error> {
        Ok(pgorm_pool::GenericClient::batch_execute(&self.0, sql).await?)
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for DatabaseTransaction<'_> {
    // #[instrument(level = "trace")]
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        cached!(self.tx(), statement, |prepared| self
            .tx()
            .execute(prepared, params)
            .await)
    }

    // #[instrument(level = "trace")]
    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        cached_once!(self.tx(), statement, |prepared| self
            .tx()
            .execute_raw(prepared, params)
            .await)
    }

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<tokio_postgres::Row, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        cached!(self.tx(), statement, |prepared| self
            .tx()
            .query_one(prepared, params)
            .await)
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<tokio_postgres::Row>, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        cached!(self.tx(), statement, |prepared| self
            .tx()
            .query_opt(prepared, params)
            .await)
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
    {
        cached!(self.tx(), statement, |prepared| self
            .tx()
            .query(prepared, params)
            .await)
    }

    // [spec:pgorm:def:exec.stream+1]    in-transaction row stream
    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        cached_once!(self.tx(), statement, |prepared| self
            .tx()
            .query_raw(prepared, params)
            .await)
    }

    async fn batch_execute(&self, sql: &str) -> Result<(), Error> {
        Ok(pgorm_pool::GenericClient::batch_execute(self.tx(), sql).await?)
    }
}
#[async_trait::async_trait]
impl TransactionTrait for DatabaseTransaction<'_> {
    async fn begin(&mut self) -> Result<DatabaseTransaction<'_>, Error> {
        Ok(DatabaseTransaction(Some(
            self.tx_mut().transaction().await?,
        )))
    }
}

// [spec:pgorm:req:conn.tx+2]    BEGIN on the pooled client
#[async_trait::async_trait]
impl TransactionTrait for DatabaseConnection {
    async fn begin(&mut self) -> Result<DatabaseTransaction<'_>, Error> {
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
