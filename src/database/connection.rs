use crate::DbErr;
use tokio_postgres::{
    Row, RowStream, ToStatement,
    types::{BorrowToSql, ToSql},
};

use super::DatabaseTransaction;

/// The generic API for a database connection that can perform query or execute statements.
/// It abstracts database connection and transaction
// [spec:pgorm:def:conn.pool.conn-trait+1]
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
}

/// Spawn database transaction
// [spec:pgorm:req:conn.tx+1]
#[async_trait::async_trait]
pub trait TransactionTrait {
    /// Execute SQL `BEGIN` transaction.
    /// Returns a Transaction that can be committed or rolled back
    async fn begin(&mut self) -> Result<DatabaseTransaction<'_>, DbErr>;
}
