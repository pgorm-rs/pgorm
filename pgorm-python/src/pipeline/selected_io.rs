use super::selected_backend::{Query, Terminal};
use crate::{
    entities,
    errors::{LifecycleError, database_error},
    runtime::Operation,
};
use pyo3::{
    prelude::*,
    types::{PyList, PyTuple},
};
use pyo3_async_runtimes::tokio::future_into_py;

// [spec:pgorm:req:python.pipeline]
pub(super) fn read<'py>(
    py: Python<'py>,
    connection: &Bound<'_, PyAny>,
    query: Query,
    terminal: Terminal,
) -> PyResult<Bound<'py, PyAny>> {
    let state = entities::io::state(py, connection)?;
    query.compile(terminal)?;
    future_into_py(py, async move {
        let operation = Operation::begin(state.clone())?;
        let result = tokio::select! {
            _ = state.cancelled.cancelled() => return Err(LifecycleError::new_err("connection is closed")),
            _ = state.pool.cancelled.cancelled() => return Err(LifecycleError::new_err("pool is closed")),
            result = async { query.run(operation.connection()?, terminal).await.map_err(|error| database_error(error, &state.pool.secrets)) } => result,
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
                    // A one-source absent model is (None,), distinct from no result row.
                    Ok(PyTuple::new(py, row)?.into_any().unbind())
                })
                .collect::<PyResult<Vec<_>>>()?;
            match terminal {
                Terminal::All => Ok(PyList::new(py, rows)?.into_any().unbind()),
                _ => Ok(rows.into_iter().next().unwrap_or_else(|| py.None())),
            }
        })
    })
}
