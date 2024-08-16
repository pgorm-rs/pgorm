use crate::{
    error::*, AccessMode, ConnectionTrait, DatabaseTransaction, ExecResult, IsolationLevel, PgConnection, QueryResult, Statement, StatementBuilder, StreamTrait, TransactionError, TransactionTrait
};
use sea_query::{PostgresQueryBuilder, QueryBuilder};
use std::{future::Future, pin::Pin};
use tracing::instrument;
use url::Url;

/// Handle a database connection depending on the backend enabled by the feature
/// flags. This creates a database pool. This will be `Clone` unless the feature
/// flag `mock` is enabled.
#[cfg_attr(not(feature = "mock"), derive(Clone))]
pub enum DatabaseConnection {
    /// Create a PostgreSQL database connection and pool
    #[cfg(feature = "sqlx-postgres")]
    SqlxPostgresPoolConnection(crate::SqlxPostgresPoolConnection),

    /// The connection to the database has been severed
    Disconnected,
}

/// The same as a [DatabaseConnection]
pub type DbConn = DatabaseConnection;

impl Default for DatabaseConnection {
    fn default() -> Self {
        Self::Disconnected
    }
}

/// The type of database backend for real world databases.
/// This is enabled by feature flags as specified in the crate documentation
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DatabaseBackend {
    /// A PostgreSQL backend
    Postgres,
}

/// The same as [DatabaseBackend] just shorter :)
pub type DbBackend = DatabaseBackend;

#[derive(Debug)]
pub(crate) enum InnerConnection {
    #[cfg(feature = "sqlx-postgres")]
    Postgres(PgConnection),
}

impl std::fmt::Debug for DatabaseConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                #[cfg(feature = "sqlx-postgres")]
                Self::SqlxPostgresPoolConnection(_) => "SqlxPostgresPoolConnection",
                Self::Disconnected => "Disconnected",
            }
        )
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for DatabaseConnection {
    fn get_database_backend(&self) -> DbBackend {
        match self {
            #[cfg(feature = "sqlx-postgres")]
            DatabaseConnection::SqlxPostgresPoolConnection(_) => DbBackend::Postgres,
            DatabaseConnection::Disconnected => panic!("Disconnected"),
        }
    }

    #[instrument(level = "trace")]
    #[allow(unused_variables)]
    async fn execute(&self, stmt: Statement) -> Result<ExecResult, DbErr> {
        match self {
            #[cfg(feature = "sqlx-postgres")]
            DatabaseConnection::SqlxPostgresPoolConnection(conn) => conn.execute(stmt).await,
            DatabaseConnection::Disconnected => Err(conn_err("Disconnected")),
        }
    }

    #[instrument(level = "trace")]
    #[allow(unused_variables)]
    async fn execute_unprepared(&self, sql: &str) -> Result<ExecResult, DbErr> {
        match self {
            #[cfg(feature = "sqlx-postgres")]
            DatabaseConnection::SqlxPostgresPoolConnection(conn) => {
                conn.execute_unprepared(sql).await
            }
            DatabaseConnection::Disconnected => Err(conn_err("Disconnected")),
        }
    }

    #[instrument(level = "trace")]
    #[allow(unused_variables)]
    async fn query_one(&self, stmt: Statement) -> Result<Option<QueryResult>, DbErr> {
        match self {
            #[cfg(feature = "sqlx-postgres")]
            DatabaseConnection::SqlxPostgresPoolConnection(conn) => conn.query_one(stmt).await,
            DatabaseConnection::Disconnected => Err(conn_err("Disconnected")),
        }
    }

    #[instrument(level = "trace")]
    #[allow(unused_variables)]
    async fn query_all(&self, stmt: Statement) -> Result<Vec<QueryResult>, DbErr> {
        match self {
            #[cfg(feature = "sqlx-postgres")]
            DatabaseConnection::SqlxPostgresPoolConnection(conn) => conn.query_all(stmt).await,
            DatabaseConnection::Disconnected => Err(conn_err("Disconnected")),
        }
    }

    #[cfg(feature = "mock")]
    fn is_mock_connection(&self) -> bool {
        false
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
            match self {
                #[cfg(feature = "sqlx-postgres")]
                DatabaseConnection::SqlxPostgresPoolConnection(conn) => conn.stream(stmt).await,
                DatabaseConnection::Disconnected => Err(conn_err("Disconnected")),
            }
        })
    }
}

#[async_trait::async_trait]
impl TransactionTrait for DatabaseConnection {
    #[instrument(level = "trace")]
    async fn begin(&self) -> Result<DatabaseTransaction, DbErr> {
        match self {
            #[cfg(feature = "sqlx-postgres")]
            DatabaseConnection::SqlxPostgresPoolConnection(conn) => conn.begin(None, None).await,
            DatabaseConnection::Disconnected => Err(conn_err("Disconnected")),
        }
    }

    #[instrument(level = "trace")]
    async fn begin_with_config(
        &self,
        _isolation_level: Option<IsolationLevel>,
        _access_mode: Option<AccessMode>,
    ) -> Result<DatabaseTransaction, DbErr> {
        match self {
            #[cfg(feature = "sqlx-postgres")]
            DatabaseConnection::SqlxPostgresPoolConnection(conn) => {
                conn.begin(_isolation_level, _access_mode).await
            }
            DatabaseConnection::Disconnected => Err(conn_err("Disconnected")),
        }
    }

    /// Execute the function inside a transaction.
    /// If the function returns an error, the transaction will be rolled back. If it does not return an error, the transaction will be committed.
    #[instrument(level = "trace", skip(_callback))]
    async fn transaction<F, T, E>(&self, _callback: F) -> Result<T, TransactionError<E>>
    where
        F: for<'c> FnOnce(
                &'c DatabaseTransaction,
            ) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'c>>
            + Send,
        T: Send,
        E: std::error::Error + Send,
    {
        match self {
            #[cfg(feature = "sqlx-postgres")]
            DatabaseConnection::SqlxPostgresPoolConnection(conn) => {
                conn.transaction(_callback, None, None).await
            }
            
            DatabaseConnection::Disconnected => Err(conn_err("Disconnected").into()),
        }
    }

    /// Execute the function inside a transaction.
    /// If the function returns an error, the transaction will be rolled back. If it does not return an error, the transaction will be committed.
    #[instrument(level = "trace", skip(_callback))]
    async fn transaction_with_config<F, T, E>(
        &self,
        _callback: F,
        _isolation_level: Option<IsolationLevel>,
        _access_mode: Option<AccessMode>,
    ) -> Result<T, TransactionError<E>>
    where
        F: for<'c> FnOnce(
                &'c DatabaseTransaction,
            ) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'c>>
            + Send,
        T: Send,
        E: std::error::Error + Send,
    {
        match self {
            
            #[cfg(feature = "sqlx-postgres")]
            DatabaseConnection::SqlxPostgresPoolConnection(conn) => {
                conn.transaction(_callback, _isolation_level, _access_mode)
                    .await
            }
            DatabaseConnection::Disconnected => Err(conn_err("Disconnected").into()),
        }
    }
}

impl DatabaseConnection {
    /// Sets a callback to metric this connection
    pub fn set_metric_callback<F>(&mut self, _callback: F)
    where
        F: Fn(&crate::metric::Info<'_>) + Send + Sync + 'static,
    {
        match self {
            #[cfg(feature = "sqlx-postgres")]
            DatabaseConnection::SqlxPostgresPoolConnection(conn) => {
                conn.set_metric_callback(_callback)
            }
            _ => {}
        }
    }

    /// Checks if a connection to the database is still valid.
    pub async fn ping(&self) -> Result<(), DbErr> {
        match self {
            #[cfg(feature = "sqlx-postgres")]
            DatabaseConnection::SqlxPostgresPoolConnection(conn) => conn.ping().await,
            DatabaseConnection::Disconnected => Err(conn_err("Disconnected")),
        }
    }

    /// Explicitly close the database connection
    pub async fn close(self) -> Result<(), DbErr> {
        match self {
            #[cfg(feature = "sqlx-postgres")]
            DatabaseConnection::SqlxPostgresPoolConnection(conn) => conn.close().await,
            DatabaseConnection::Disconnected => Err(conn_err("Disconnected")),
        }
    }
}

impl DatabaseConnection {
    /// Get [sqlx::PgPool]
    ///
    /// # Panics
    ///
    /// Panics if [DbConn] is not a Postgres connection.
    #[cfg(feature = "sqlx-postgres")]
    pub fn get_postgres_connection_pool(&self) -> &crate::PgPoolWrapper {
        match self {
            DatabaseConnection::SqlxPostgresPoolConnection(conn) => &conn.pool,
            _ => panic!("Not Postgres Connection"),
        }
    }
}

impl DbBackend {
    /// Check if the URI is the same as the specified database backend.
    /// Returns true if they match.
    ///
    /// # Panics
    ///
    /// Panics if `base_url` cannot be parsed as `Url`.
    pub fn is_prefix_of(self, base_url: &str) -> bool {
        let base_url_parsed = Url::parse(base_url).expect("Fail to parse database URL");
        match self {
            Self::Postgres => {
                base_url_parsed.scheme() == "postgres" || base_url_parsed.scheme() == "postgresql"
            }
        }
    }

    /// Build an SQL [Statement]
    pub fn build<S>(&self, statement: &S) -> Statement
    where
        S: StatementBuilder,
    {
        statement.build(self)
    }

    /// A helper for building SQL queries
    pub fn get_query_builder(&self) -> Box<dyn QueryBuilder> {
        match self {
            Self::Postgres => Box::new(PostgresQueryBuilder),
        }
    }

    /// Check if the database supports `RETURNING` syntax on insert and update
    pub fn support_returning(&self) -> bool {
        match self {
            Self::Postgres => true,
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
