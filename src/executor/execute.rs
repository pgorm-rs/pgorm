/// Defines the result of executing an operation
#[derive(Debug)]
pub struct ExecResult {
    /// The type of result from the execution depending on the feature flag enabled
    /// to choose a database backend
    pub(crate) result: ExecResultHolder,
}

/// Holds a result depending on the database backend chosen by the feature flag
#[allow(clippy::enum_variant_names)]
#[derive(Debug)]
pub(crate) enum ExecResultHolder {
    /// Holds the result of executing an operation on a PostgreSQL database
    #[cfg(feature = "sqlx-postgres")]
    SqlxPostgres(sqlx::postgres::PgQueryResult),
}

// ExecResult //

impl ExecResult {
    /// Get the last id after `AUTOINCREMENT` is done on the primary key
    ///
    /// # Panics
    ///
    /// Postgres does not support retrieving last insert id this way except through `RETURNING` clause
    pub fn last_insert_id(&self) -> u64 {
        match &self.result {
            #[cfg(feature = "sqlx-postgres")]
            ExecResultHolder::SqlxPostgres(_) => {
                panic!("Should not retrieve last_insert_id this way")
            }
            #[allow(unreachable_patterns)]
            _ => unreachable!(),
        }
    }

    /// Get the number of rows affected by the operation
    pub fn rows_affected(&self) -> u64 {
        match &self.result {
            #[cfg(feature = "sqlx-postgres")]
            ExecResultHolder::SqlxPostgres(result) => result.rows_affected(),
            #[allow(unreachable_patterns)]
            _ => unreachable!(),
        }
    }
}
