/// Defines the result of executing an operation
#[derive(Debug)]
#[repr(transparent)]
pub struct ExecResult {
    /// The type of result from the execution depending on the feature flag enabled
    /// to choose a database backend
    pub(crate) result: ExecResultHolder,
}

/// Holds the result of executing an operation on a PostgreSQL database
#[derive(Debug)]
#[repr(transparent)]
pub(crate) struct ExecResultHolder(pub(crate) sqlx::postgres::PgQueryResult);

// ExecResult //

impl ExecResult {
    /// Get the number of rows affected by the operation
    pub fn rows_affected(&self) -> u64 {
        self.result.0.rows_affected()
    }
}
