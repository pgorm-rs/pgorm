use crate::Error;
use tokio_postgres::{
    Row, RowStream, Statement, ToStatement,
    types::{BorrowToSql, ToSql},
};

use super::DatabaseTransaction;

/// The SQL text behind a statement, where the statement still carries it.
///
/// [`ToStatement`] is sealed by tokio-postgres and hides which of its two forms
/// a caller passed, so code that wants to look at the SQL — to key a statement
/// cache on it, to fingerprint it, to log it — has no way to ask. This trait is
/// that question, answered by
/// each of the three types `ToStatement` admits: a `str` or `String` is the SQL
/// itself, while a prepared [`Statement`] holds only the server-side name it
/// was assigned, its parameter types, and its result columns. The text is gone
/// by then, so the answer is [`None`] rather than a reconstruction.
// [spec:pgorm:def:conn.sql-text+1]
pub trait SqlText {
    /// The statement's SQL text, or `None` when the statement no longer carries
    /// it.
    fn sql_text(&self) -> Option<&str>;
}

// [spec:pgorm:def:conn.sql-text+1]    the three shapes ToStatement admits
impl SqlText for str {
    fn sql_text(&self) -> Option<&str> {
        Some(self)
    }
}

impl SqlText for String {
    fn sql_text(&self) -> Option<&str> {
        Some(self)
    }
}

impl SqlText for Statement {
    fn sql_text(&self) -> Option<&str> {
        None
    }
}

/// The generic API for a database connection that can perform query or execute statements.
/// It abstracts database connection and transaction
///
/// The six statement methods go through the extended protocol and resolve the
/// statement through the connection's prepared-statement cache, so a SQL text
/// executed twice on one connection is parsed once. An already-prepared
/// [`Statement`] is passed through untouched. The seventh, [`batch_execute`],
/// uses the simple-query protocol and prepares nothing.
///
/// [`batch_execute`]: ConnectionTrait::batch_execute
// [spec:pgorm:def:conn.pool.conn-trait+5]
#[async_trait::async_trait]
pub trait ConnectionTrait: Sync {
    /// Execute a SQL statement with its bound parameters, returning the number
    /// of rows it affected
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync;

    /// [`ConnectionTrait::execute`] over parameters supplied as an iterator of
    /// individually-typed values rather than a slice of one trait object type
    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator;

    /// Execute a SQL statement with its bound parameters, expecting exactly one
    /// row; any other row count is an error
    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Row, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync;

    /// Execute a SQL statement with its bound parameters, expecting at most one
    /// row; more than one is an error
    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<Row>, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync;

    /// Execute a SQL statement with its bound parameters, buffering every row
    /// it returns
    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<Row>, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync;

    /// Execute a statement and return its rows as a stream, without buffering
    /// the whole result set
    // [spec:pgorm:def:exec.stream+1]
    async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, Error>
    where
        T: ?Sized + ToStatement + SqlText + Send + Sync,
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
    async fn batch_execute(&self, sql: &str) -> Result<(), Error>;
}

/// Spawn database transaction
// [spec:pgorm:req:conn.tx+2]
#[async_trait::async_trait]
pub trait TransactionTrait {
    /// Execute SQL `BEGIN` transaction.
    /// Returns a Transaction that can be committed or rolled back
    async fn begin(&mut self) -> Result<DatabaseTransaction<'_>, Error>;
}
