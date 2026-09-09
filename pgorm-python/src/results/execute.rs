use std::sync::Arc;

use pgorm::{ConnectionTrait, ValueHolder};
use pyo3::{prelude::*, types::PyList};
use pyo3_async_runtimes::tokio::future_into_py;
use tokio_postgres::types::ToSql;

use super::PyRecord;
use crate::{
    errors::{DatabaseError, LifecycleError, database_error},
    runtime::{ConnectionState, Operation},
    statements,
};

// [spec:pgorm:req:python.results]
pub(crate) fn execute<'py>(
    py: Python<'py>,
    state: Arc<ConnectionState>,
    query: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    state.pool.check_owner(py)?;
    state.ensure_open()?;
    let compiled = statements::compile(query)?;
    future_into_py(py, async move {
        let operation = Operation::begin(state.clone())?;
        let values: Vec<_> = compiled.values.0.into_iter().map(ValueHolder).collect();
        let params: Vec<_> = values.iter().map(|v| v as &(dyn ToSql + Sync)).collect();
        let result = tokio::select! {
            _ = state.cancelled.cancelled() => return Err(LifecycleError::new_err("connection is closed")),
            _ = state.pool.cancelled.cancelled() => return Err(LifecycleError::new_err("pool is closed")),
            result = operation.connection()?.execute(&compiled.sql, &params) => result,
        };
        operation.restore();
        result.map_err(|error| database_error(error, &state.pool.secrets))
    })
}

/// All three materializing terminals use the same completed Rust query result.
/// Cardinality checks never turn database/decode failures into a missing row.
pub(crate) fn fetch<'py>(
    py: Python<'py>,
    state: Arc<ConnectionState>,
    query: &Bound<'_, PyAny>,
    mode: &str,
) -> PyResult<Bound<'py, PyAny>> {
    state.pool.check_owner(py)?;
    state.ensure_open()?;
    let compiled = statements::compile(query)?;
    let mode = match mode {
        "all" => 0,
        "one" => 1,
        "optional" => 2,
        _ => {
            return Err(crate::errors::ConstructionError::new_err(
                "unknown result terminal",
            ));
        }
    };
    future_into_py(py, async move {
        let operation = Operation::begin(state.clone())?;
        let values: Vec<_> = compiled.values.0.into_iter().map(ValueHolder).collect();
        let params: Vec<_> = values.iter().map(|v| v as &(dyn ToSql + Sync)).collect();
        let result = tokio::select! {
            _ = state.cancelled.cancelled() => return Err(LifecycleError::new_err("connection is closed")),
            _ = state.pool.cancelled.cancelled() => return Err(LifecycleError::new_err("pool is closed")),
            result = operation.connection()?.query_all(&compiled.sql, &params) => result,
        };
        operation.restore();
        let rows = result.map_err(|error| database_error(error, &state.pool.secrets))?;
        if (mode == 1 && rows.len() != 1) || (mode == 2 && rows.len() > 1) {
            return Err(DatabaseError::new_err(format!(
                "expected {} row, received {}",
                if mode == 1 {
                    "exactly one"
                } else {
                    "at most one"
                },
                rows.len()
            )));
        }
        let records = rows
            .into_iter()
            .map(PyRecord::decode)
            .collect::<PyResult<Vec<_>>>()?;
        Python::attach(|py| match mode {
            0 => Ok(PyList::new(py, records)?.into_any().unbind()),
            _ => match records.into_iter().next() {
                Some(record) => Ok(Py::new(py, record)?.into_any()),
                None => Ok(py.None()),
            },
        })
    })
}
