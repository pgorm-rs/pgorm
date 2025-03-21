use crate::DbErr;
use tokio_postgres::{
    Row, ToStatement,
    types::{BorrowToSql, ToSql},
};

use super::DatabaseTransaction;

/// The generic API for a database connection that can perform query or execute statements.
/// It abstracts database connection and transaction
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

    // async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, DbErr>
    // where
    //     T: ?Sized + ToStatement,
    //     P: BorrowToSql,
    //     I: IntoIterator<Item = P>,
    //     I::IntoIter: ExactSizeIterator;
}

/// Spawn database transaction
#[async_trait::async_trait]
pub trait TransactionTrait {
    /// Execute SQL `BEGIN` transaction.
    /// Returns a Transaction that can be committed or rolled back
    async fn begin(&mut self) -> Result<DatabaseTransaction<'_>, DbErr>;

    // /// Execute SQL `BEGIN` transaction with isolation level and/or access mode.
    // /// Returns a Transaction that can be committed or rolled back
    // async fn begin_with_config(
    //     &mut self,
    //     read_only: bool,
    //     isolation_level: Option<tokio_postgres::IsolationLevel>,
    // ) -> Result<DatabaseTransaction<'_>, DbErr>;

    // Execute the function inside a transaction.
    // If the function returns an error, the transaction will be rolled back. If it does not return an error, the transaction will be committed.
    // async fn transaction<F, T, E>(&self, callback: F) -> Result<T, TransactionError<E>>
    // where
    //     F: for<'c> FnOnce(
    //             &'c DatabaseTransaction,
    //         ) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'c>>
    //         + Send,
    //     T: Send,
    //     E: std::error::Error + Send;

    // /// Execute the function inside a transaction with isolation level and/or access mode.
    // /// If the function returns an error, the transaction will be rolled back. If it does not return an error, the transaction will be committed.
    // async fn transaction_with_config<F, T, E>(
    //     &self,
    //     callback: F,
    //     isolation_level: Option<IsolationLevel>,
    //     access_mode: Option<AccessMode>,
    // ) -> Result<T, TransactionError<E>>
    // where
    //     F: for<'c> FnOnce(
    //             &'c DatabaseTransaction,
    //         ) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'c>>
    //         + Send,
    //     T: Send,
    //     E: std::error::Error + Send;
}
