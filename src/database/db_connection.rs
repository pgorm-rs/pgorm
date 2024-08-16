use crate::{
    error::*, AccessMode, ConnectionTrait, DatabaseTransaction, ExecResult, IsolationLevel,
    PgConnection, QueryResult, Statement, StatementBuilder, StreamTrait, TransactionError,
    TransactionTrait,
};
use sea_query::{PostgresQueryBuilder, QueryBuilder};
use std::{future::Future, pin::Pin};
use tracing::instrument;
use url::Url;

/// Handle a database connection depending on the backend enabled by the feature
/// flags. This creates a database pool.
#[derive(Clone)]
#[repr(transparent)]
pub struct DatabaseConnection(pub Option<crate::SqlxPostgresPoolConnection>);

impl Default for DatabaseConnection {
    fn default() -> Self {
        Self(None)
    }
}

#[derive(Debug)]
pub(crate) struct InnerConnection(pub(crate) PgConnection);

impl std::fmt::Debug for DatabaseConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self.0.as_ref() {
                Some(_) => "SqlxPostgresPoolConnection",
                None => "Disconnected",
            }
        )
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for DatabaseConnection {
    #[instrument(level = "trace")]
    #[allow(unused_variables)]
    async fn execute(&self, stmt: Statement) -> Result<ExecResult, DbErr> {
        match self.0.as_ref() {
            Some(conn) => conn.execute(stmt).await,
            None => Err(conn_err("Disconnected")),
        }
    }

    #[instrument(level = "trace")]
    #[allow(unused_variables)]
    async fn execute_unprepared(&self, sql: &str) -> Result<ExecResult, DbErr> {
        match self.0.as_ref() {
            Some(conn) => conn.execute_unprepared(sql).await,
            None => Err(conn_err("Disconnected")),
        }
    }

    #[instrument(level = "trace")]
    #[allow(unused_variables)]
    async fn query_one(&self, stmt: Statement) -> Result<Option<QueryResult>, DbErr> {
        match self.0.as_ref() {
            #[cfg(feature = "sqlx-postgres")]
            Some(conn) => conn.query_one(stmt).await,
            None => Err(conn_err("Disconnected")),
        }
    }

    #[instrument(level = "trace")]
    #[allow(unused_variables)]
    async fn query_all(&self, stmt: Statement) -> Result<Vec<QueryResult>, DbErr> {
        match self.0.as_ref() {
            #[cfg(feature = "sqlx-postgres")]
            Some(conn) => conn.query_all(stmt).await,
            None => Err(conn_err("Disconnected")),
        }
    }
}

#[async_trait::async_trait]
impl StreamTrait for DatabaseConnection {
    type Stream<'a> = crate::QueryStream;

    #[instrument(level = "trace")]
    #[allow(unused_variables)]
    fn stream<'a>(
        &'a self,
        stmt: Statement,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Stream<'a>, DbErr>> + 'a + Send>> {
        Box::pin(async move {
            match self.0.as_ref() {
                #[cfg(feature = "sqlx-postgres")]
                Some(conn) => conn.stream(stmt).await,
                None => Err(conn_err("Disconnected")),
            }
        })
    }
}

#[async_trait::async_trait]
impl TransactionTrait for DatabaseConnection {
    #[instrument(level = "trace")]
    async fn begin(&self) -> Result<DatabaseTransaction, DbErr> {
        match self.0.as_ref() {
            #[cfg(feature = "sqlx-postgres")]
            Some(conn) => conn.begin(None, None).await,
            None => Err(conn_err("Disconnected")),
        }
    }

    #[instrument(level = "trace")]
    async fn begin_with_config(
        &self,
        isolation_level: Option<IsolationLevel>,
        access_mode: Option<AccessMode>,
    ) -> Result<DatabaseTransaction, DbErr> {
        match self.0.as_ref() {
            #[cfg(feature = "sqlx-postgres")]
            Some(conn) => conn.begin(isolation_level, access_mode).await,
            None => Err(conn_err("Disconnected")),
        }
    }

    /// Execute the function inside a transaction.
    /// If the function returns an error, the transaction will be rolled back. If it does not return an error, the transaction will be committed.
    #[instrument(level = "trace", skip(callback))]
    async fn transaction<F, T, E>(&self, callback: F) -> Result<T, TransactionError<E>>
    where
        F: for<'c> FnOnce(
                &'c DatabaseTransaction,
            ) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'c>>
            + Send,
        T: Send,
        E: std::error::Error + Send,
    {
        match self.0.as_ref() {
            #[cfg(feature = "sqlx-postgres")]
            Some(conn) => conn.transaction(callback, None, None).await,

            None => Err(conn_err("Disconnected").into()),
        }
    }

    /// Execute the function inside a transaction.
    /// If the function returns an error, the transaction will be rolled back. If it does not return an error, the transaction will be committed.
    #[instrument(level = "trace", skip(callback))]
    async fn transaction_with_config<F, T, E>(
        &self,
        callback: F,
        isolation_level: Option<IsolationLevel>,
        access_mode: Option<AccessMode>,
    ) -> Result<T, TransactionError<E>>
    where
        F: for<'c> FnOnce(
                &'c DatabaseTransaction,
            ) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'c>>
            + Send,
        T: Send,
        E: std::error::Error + Send,
    {
        match self.0.as_ref() {
            #[cfg(feature = "sqlx-postgres")]
            Some(conn) => {
                conn.transaction(callback, isolation_level, access_mode)
                    .await
            }
            None => Err(conn_err("Disconnected").into()),
        }
    }
}

impl DatabaseConnection {
    /// Sets a callback to metric this connection
    pub fn set_metric_callback<F>(&mut self, _callback: F)
    where
        F: Fn(&crate::metric::Info<'_>) + Send + Sync + 'static,
    {
        match self.0.as_mut() {
            Some(conn) => conn.set_metric_callback(_callback),
            _ => {}
        }
    }

    /// Checks if a connection to the database is still valid.
    pub async fn ping(&self) -> Result<(), DbErr> {
        match self.0.as_ref() {
            Some(conn) => conn.ping().await,
            None => Err(conn_err("Disconnected")),
        }
    }

    /// Explicitly close the database connection
    pub async fn close(mut self) -> Result<(), DbErr> {
        match self.0.take() {
            Some(conn) => conn.close().await,
            None => Err(conn_err("Disconnected")),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::DatabaseConnection;

    #[test]
    fn assert_database_connection_traits() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<DatabaseConnection>();
    }
}
