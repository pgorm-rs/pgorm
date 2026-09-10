use super::Database;
use crate::{
    errors::{ConstructionError, LifecycleError, database_error},
    runtime::{ConnectionState, NativeConnection, Operation},
    transactions::{
        NativeTransaction, State,
        work::{Output, Work},
    },
};
use pyo3::prelude::*;
use std::sync::Arc;

pub(crate) enum Target {
    Connection(Arc<ConnectionState>),
    Transaction(Arc<State>),
}

impl Target {
    pub(crate) fn extract(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let native = value.getattr("_native").unwrap_or_else(|_| value.clone());
        let target = if let Ok(connection) = native.extract::<PyRef<'_, NativeConnection>>() {
            Self::Connection(connection.state.clone())
        } else if let Ok(transaction) = native.extract::<PyRef<'_, NativeTransaction>>() {
            Self::Transaction(transaction.state.clone())
        } else {
            return Err(ConstructionError::new_err(
                "execution requires an acquired pgorm Connection or Transaction",
            ));
        };
        target.check(py)?;
        Ok(target)
    }

    pub(crate) fn check(&self, py: Python<'_>) -> PyResult<()> {
        match self {
            Self::Connection(state) => {
                state.pool.check_owner(py)?;
                state.ensure_open()
            }
            Self::Transaction(state) => state.check(py),
        }
    }

    pub(crate) async fn run(self, work: Work) -> PyResult<Output> {
        match self {
            Self::Transaction(state) => state.run(work).await,
            Self::Connection(state) => {
                let operation = Operation::begin(state.clone())?;
                let result = tokio::select! {
                    _ = state.cancelled.cancelled() => return Err(LifecycleError::new_err("connection is closed")),
                    _ = state.pool.cancelled.cancelled() => return Err(LifecycleError::new_err("pool is closed")),
                    result = work.run(Database::Connection(operation.connection()?)) => result,
                };
                operation.restore();
                result.map_err(|error| database_error(error, &state.pool.secrets))
            }
        }
    }
}
