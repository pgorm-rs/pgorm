use super::backend::{CursorPlan, Query};
use crate::{
    entities,
    errors::{ConstructionError, LifecycleError, database_error},
    runtime::Operation,
};
use pyo3::{
    IntoPyObjectExt,
    prelude::*,
    types::{PyList, PyTuple},
};
use pyo3_async_runtimes::tokio::future_into_py;

pub(crate) enum Read {
    All,
    Optional,
    Cursor(CursorPlan),
}

// [spec:pgorm:req:python.graph]
pub(crate) fn read<'py>(
    py: Python<'py>,
    connection: &Bound<'_, PyAny>,
    query: Query,
    terminal: Read,
) -> PyResult<Bound<'py, PyAny>> {
    let state = entities::io::state(py, connection)?;
    let optional = matches!(terminal, Read::Optional);
    if query.compile(optional).1.0.len() > 65535 {
        return Err(ConstructionError::new_err(
            "PostgreSQL supports at most 65535 query parameters",
        ));
    }
    future_into_py(py, async move {
        let operation = Operation::begin(state.clone())?;
        let result = tokio::select! {
            _ = state.cancelled.cancelled() => return Err(LifecycleError::new_err("connection is closed")),
            _ = state.pool.cancelled.cancelled() => return Err(LifecycleError::new_err("pool is closed")),
            result = async {
                match terminal {
                    Read::Cursor(plan) => query.cursor(operation.connection()?, plan).await.map_err(|error| database_error(error, &state.pool.secrets)),
                    _ => query.run(operation.connection()?, optional).await.map_err(|error| database_error(error, &state.pool.secrets)),
                }
            } => result,
        };
        operation.restore();
        let rows = result?;
        Python::attach(|py| {
            let rows = rows
                .into_iter()
                .map(|row| {
                    for model in row.iter().flatten() {
                        model.validate(py)?;
                    }
                    if row.len() == 1 {
                        row.into_iter().next().flatten().into_py_any(py)
                    } else {
                        Ok(PyTuple::new(py, row)?.into_any().unbind())
                    }
                })
                .collect::<PyResult<Vec<_>>>()?;
            if optional {
                Ok(rows.into_iter().next().unwrap_or_else(|| py.None()))
            } else {
                Ok(PyList::new(py, rows)?.into_any().unbind())
            }
        })
    })
}
