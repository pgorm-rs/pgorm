use thiserror::Error;
use tokio_postgres::error::SqlState;

/// An error from unsuccessful database operations
// [spec:pgorm:def:error.model+3]
#[derive(Error, Debug)]
pub enum DbErr {
    /// Postgres error
    #[error("Postgres Error: {0:?} {:?}", .0.as_db_error())]
    Postgres(#[from] tokio_postgres::Error),
    /// Pool error
    #[error("Pool Error: {0}")]
    Pool(#[from] pgorm_pool::PoolError),
    /// Runtime type conversion error
    #[error("Error converting `{from}` into `{into}`: {source}")]
    TryIntoErr {
        /// From type
        from: &'static str,
        /// Into type
        into: &'static str,
        /// TryError
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// An error occurred while performing a query
    #[error("Query Error: {0}")]
    Query(#[source] RuntimeErr),
    /// Type error: the specified type cannot be converted from u64. This is not a runtime error.
    #[error("Type '{0}' cannot be converted from u64")]
    ConvertFromU64(&'static str),
    /// After an insert statement it was impossible to retrieve the last_insert_id
    #[error("Failed to unpack last_insert_id")]
    UnpackInsertId,
    /// A primary-key column of the model is `NotSet`, so there is nothing to
    /// narrow the statement to a single row
    #[error("A primary key value is not set")]
    PrimaryKeyNotSet,
    /// Thrown by `TryFrom<ActiveModel>`, which assumes all attributes are set/unchanged
    #[error("Attribute {0} is NotSet")]
    AttrNotSet(String),
    /// Error occurred while parsing value as target type
    #[error("Type Error: {0}")]
    Type(String),
    /// Error occurred while parsing json value as target type
    #[error("Json Error: {0}")]
    Json(String),
    /// The record was not found in the database
    #[error("No records were returned for the given query")]
    RecordNotFound,
    /// None of the records are inserted,
    /// that probably means all of them conflict with existing records in the table
    #[error("None of the records are inserted")]
    RecordNotInserted,
    /// None of the records are updated, that means a WHERE condition has no matches.
    /// May be the table is empty or the record does not exist
    #[error("None of the records are updated")]
    RecordNotUpdated,
    /// A custom error
    #[error("Custom Error: {0}")]
    Custom(String),
}

/// Runtime error
// [spec:pgorm:def:error.model.runtime+2]
#[derive(Error, Debug)]
pub enum RuntimeErr {
    /// Error generated from within pgorm
    #[error("{0}")]
    Internal(String),
}

// [spec:pgorm:def:error.model+3]    Display-string equality
impl PartialEq for DbErr {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Eq for DbErr {}

/// Error during `impl FromStr for Entity::Column`
#[derive(Error, Debug)]
#[error("Failed to match \"{0}\" as Column")]
pub struct ColumnFromStrErr(pub String);

#[allow(dead_code)]
pub(crate) fn query_err<T>(s: T) -> DbErr
where
    T: ToString,
{
    DbErr::Query(RuntimeErr::Internal(s.to_string()))
}

#[allow(dead_code)]
pub(crate) fn type_err<T>(s: T) -> DbErr
where
    T: ToString,
{
    DbErr::Type(s.to_string())
}

#[allow(dead_code)]
pub(crate) fn json_err<T>(s: T) -> DbErr
where
    T: ToString,
{
    DbErr::Json(s.to_string())
}

/// An error from unsuccessful SQL query
// [spec:pgorm:sem:error.model.sql-class+2]
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SqlErr {
    /// Error for duplicate record in unique field or primary key field
    #[error("Unique Constraint Violated: {0}")]
    UniqueConstraintViolation(String),
    /// Error for Foreign key constraint
    #[error("Foreign Key Constraint Violated: {0}")]
    ForeignKeyConstraintViolation(String),
}

/// An error after which re-running the whole transaction may succeed.
///
/// Implemented for [`DbErr`]; implement it for a domain error type to use that
/// type as the closure error of
/// [`DatabaseConnection::transaction_with_retry`](crate::DatabaseConnection::transaction_with_retry).
// [spec:pgorm:sem:conn.tx.retry]    retryability predicate
pub trait RetryableError {
    /// Whether the transaction that produced this error is worth retrying.
    fn is_retryable(&self) -> bool;
}

// [spec:pgorm:sem:conn.tx.retry]
impl RetryableError for DbErr {
    fn is_retryable(&self) -> bool {
        DbErr::is_retryable(self)
    }
}

impl DbErr {
    /// Whether this is a transaction-rollback error PostgreSQL expects the
    /// client to retry: SQLSTATE `40001` (`serialization_failure`) or `40P01`
    /// (`deadlock_detected`).
    // [spec:pgorm:sem:conn.tx.retry]    SQLSTATE classification
    pub fn is_retryable(&self) -> bool {
        let DbErr::Postgres(error) = self else {
            return false;
        };
        let Some(db_error) = error.as_db_error() else {
            return false;
        };
        let code = db_error.code();
        *code == SqlState::T_R_SERIALIZATION_FAILURE || *code == SqlState::T_R_DEADLOCK_DETECTED
    }

    /// Classify a [`DbErr`] as a [`SqlErr`], returning `None` when the error is
    /// not a recognised constraint violation.
    // [spec:pgorm:sem:error.model.sql-class+2]    classifier entry point
    pub fn sql_err(&self) -> Option<SqlErr> {
        let db_error = match self {
            DbErr::Postgres(e) => e.as_db_error()?,
            _ => return None,
        };
        let code = db_error.code();
        let message = db_error.message().to_owned();
        if *code == SqlState::UNIQUE_VIOLATION {
            Some(SqlErr::UniqueConstraintViolation(message))
        } else if *code == SqlState::FOREIGN_KEY_VIOLATION {
            Some(SqlErr::ForeignKeyConstraintViolation(message))
        } else {
            None
        }
    }
}
