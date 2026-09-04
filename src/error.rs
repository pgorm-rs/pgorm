use tokio_postgres::error::SqlState;

/// An error from unsuccessful database operations
// [spec:pgorm:def:error.model+6]
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Postgres error
    #[error("Postgres Error: {0:?} {db:?}", db = .0.as_db_error())]
    Postgres(#[from] tokio_postgres::Error),
    /// Pool error
    #[error("Pool Error: {0}")]
    Pool(#[from] pgorm_pool::PoolError),
    /// Runtime type conversion error
    #[error("Error converting `{from}` into `{into}`: {source}")]
    Conversion {
        /// From type
        from: &'static str,
        /// Into type
        into: &'static str,
        /// The conversion failure being reported
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// An error occurred while performing a query
    #[error("Query Error: {0}")]
    Query(#[source] RuntimeError),
    /// Type error: the specified type cannot be converted from u64. This is not a runtime error.
    #[error("Type '{0}' cannot be converted from u64")]
    ConvertFromU64(&'static str),
    /// The inserted row's primary key could not be decoded from `RETURNING`
    #[error("Failed to unpack the inserted primary key")]
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
    /// A decode target does not match the statement it would decode
    #[error("Verification Error: {0}")]
    Verify(#[from] VerifyError),
    /// A custom error
    #[error("Custom Error: {0}")]
    Custom(String),
}

/// Why a `FromQueryResult` target does not match the statement it would decode,
/// as reported by [`VerifyStatement::verify`](crate::VerifyStatement::verify).
///
/// Each variant names the target, the column at fault, and what the statement
/// says about that column, so the message identifies the field to change
/// without a second lookup.
// [spec:pgorm:req:exec.verify.errors]
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VerifyError {
    /// The target reads a column the statement does not return
    #[error(
        "`{target}` reads column `{column}`, which the statement does not return; it returns: {returned}"
    )]
    ColumnMissing {
        /// The decode target being verified
        target: &'static str,
        /// The column name the target looks up
        column: &'static str,
        /// The column names the statement does return, in order
        returned: String,
    },
    /// The statement returns the column, but its PostgreSQL type is not one the
    /// field's Rust type can decode
    #[error(
        "`{target}` decodes column `{column}` as `{rust_type}`, which cannot read PostgreSQL type `{pg_type}`"
    )]
    ColumnType {
        /// The decode target being verified
        target: &'static str,
        /// The column name the target looks up
        column: &'static str,
        /// The Rust type the target decodes the column into
        rust_type: &'static str,
        /// The PostgreSQL type the statement returns for that column
        pg_type: String,
    },
    /// The target reports no columns, so there is nothing to check it against
    #[error(
        "`{target}` reports no columns: a hand-written `FromQueryResult` impl that does not override `expected_columns` cannot be verified"
    )]
    Unreflected {
        /// The decode target being verified
        target: &'static str,
    },
}

/// The result of a fallible pgorm operation, defaulting to [`Error`].
///
/// The default type parameter lets callers write `pgorm::Result<Model>` for
/// the common case while still naming a foreign error type — as
/// [`DatabaseConnection::transaction`](crate::DatabaseConnection::transaction)
/// does — where the closure carries its own.
///
/// ```
/// fn first_cake_id() -> pgorm::Result<i32> {
///     Err(pgorm::Error::RecordNotFound)
/// }
///
/// assert!(first_cake_id().is_err());
/// ```
// [spec:pgorm:def:error.model+6]    crate-root Result alias
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Runtime error
// [spec:pgorm:def:error.model.runtime+3]
#[derive(thiserror::Error, Debug)]
pub enum RuntimeError {
    /// Error generated from within pgorm
    #[error("{0}")]
    Internal(String),
}

// [spec:pgorm:def:error.model+6]    Display-string equality
impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Eq for Error {}

/// Error during `impl FromStr for Entity::Column`
#[derive(thiserror::Error, Debug)]
#[error("Failed to match \"{0}\" as Column")]
pub struct ColumnFromStrError(pub String);

#[allow(dead_code)]
pub(crate) fn query_err<T>(s: T) -> Error
where
    T: ToString,
{
    Error::Query(RuntimeError::Internal(s.to_string()))
}

#[allow(dead_code)]
pub(crate) fn type_err<T>(s: T) -> Error
where
    T: ToString,
{
    Error::Type(s.to_string())
}

#[allow(dead_code)]
pub(crate) fn json_err<T>(s: T) -> Error
where
    T: ToString,
{
    Error::Json(s.to_string())
}

#[allow(dead_code)]
pub(crate) fn primary_key_type_err(table: &str, err: pgorm_query::ValueTupleError) -> Error {
    Error::Type(format!(
        "primary key of `{table}` does not match its declared `ValueType`: {err}"
    ))
}

/// An error from unsuccessful SQL query
// [spec:pgorm:sem:error.model.sql-class+3]
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SqlError {
    /// Error for duplicate record in unique field or primary key field
    #[error("Unique Constraint Violated: {0}")]
    UniqueConstraintViolation(String),
    /// Error for Foreign key constraint
    #[error("Foreign Key Constraint Violated: {0}")]
    ForeignKeyConstraintViolation(String),
}

/// An error after which re-running the whole transaction may succeed.
///
/// Implemented for [`Error`]; implement it for a domain error type to use that
/// type as the closure error of
/// [`DatabaseConnection::transaction_with_retry`](crate::DatabaseConnection::transaction_with_retry).
// [spec:pgorm:sem:conn.tx.retry+1]    retryability predicate
pub trait RetryableError {
    /// Whether the transaction that produced this error is worth retrying.
    fn is_retryable(&self) -> bool;
}

// [spec:pgorm:sem:conn.tx.retry+1]
impl RetryableError for Error {
    fn is_retryable(&self) -> bool {
        Error::is_retryable(self)
    }
}

impl Error {
    /// Whether this is a transaction-rollback error PostgreSQL expects the
    /// client to retry: SQLSTATE `40001` (`serialization_failure`) or `40P01`
    /// (`deadlock_detected`).
    // [spec:pgorm:sem:conn.tx.retry+1]    SQLSTATE classification
    pub fn is_retryable(&self) -> bool {
        let Error::Postgres(error) = self else {
            return false;
        };
        let Some(db_error) = error.as_db_error() else {
            return false;
        };
        let code = db_error.code();
        *code == SqlState::T_R_SERIALIZATION_FAILURE || *code == SqlState::T_R_DEADLOCK_DETECTED
    }

    /// Classify an [`Error`] as a [`SqlError`], returning `None` when the error is
    /// not a recognised constraint violation.
    // [spec:pgorm:sem:error.model.sql-class+3]    classifier entry point
    pub fn sql_error(&self) -> Option<SqlError> {
        let db_error = match self {
            Error::Postgres(e) => e.as_db_error()?,
            _ => return None,
        };
        let code = db_error.code();
        let message = db_error.message().to_owned();
        if *code == SqlState::UNIQUE_VIOLATION {
            Some(SqlError::UniqueConstraintViolation(message))
        } else if *code == SqlState::FOREIGN_KEY_VIOLATION {
            Some(SqlError::ForeignKeyConstraintViolation(message))
        } else {
            None
        }
    }
}
