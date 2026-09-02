use crate::DbErr;
use tokio_postgres::{
    Row, RowStream, ToStatement,
    types::{BorrowToSql, ToSql},
};

use super::DatabaseTransaction;

/// The generic API for a database connection that can perform query or execute statements.
/// It abstracts database connection and transaction
// [spec:pgorm:def:conn.pool.conn-trait+2]
#[async_trait::async_trait]
pub trait ConnectionTrait: Sync {
    /// Execute a [Statement]
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync;

    /// Execute a unprepared [Statement]
    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator;

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Row, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync;

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync;

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync;

    /// Execute a statement and return its rows as a stream, without buffering
    /// the whole result set
    // [spec:pgorm:def:exec.stream]
    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator;

    /// Send `sql` through the simple-query protocol, running every
    /// `;`-separated statement it contains.
    ///
    /// This is the only method that accepts a multi-statement string: the
    /// others go through the extended protocol, where a prepared statement
    /// carries exactly one command and PostgreSQL answers a second one with
    /// *cannot insert multiple commands into a prepared statement*. Execution
    /// stops at the first statement that fails, and outside an explicit
    /// transaction PostgreSQL runs the whole string as one implicit
    /// transaction, so that failure discards the statements before it too.
    ///
    /// It takes no parameters and yields no rows. Nothing is prepared, so the
    /// statement cache is bypassed by construction. It is meant for DDL,
    /// migration, and fixture surfaces, where a script is the unit of work.
    ///
    /// Because there is no parameter binding, any value has to be interpolated
    /// into `sql` by the caller — build these strings from trusted input only,
    /// and reach for [`ConnectionTrait::execute`] the moment user data is
    /// involved.
    async fn batch_execute(&self, sql: &str) -> Result<(), DbErr>;
}

/// Spawn database transaction
// [spec:pgorm:req:conn.tx+1]
#[async_trait::async_trait]
pub trait TransactionTrait {
    /// Execute SQL `BEGIN` transaction.
    /// Returns a Transaction that can be committed or rolled back
    async fn begin(&mut self) -> Result<DatabaseTransaction<'_>, DbErr>;
}
