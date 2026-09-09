use pyo3::{IntoPyObjectExt, prelude::*, types::PyList};
use pyo3_async_runtimes::tokio::future_into_py;
use std::sync::Arc;

use super::{
    PyEntityModel,
    backend::{Active, Select, Terminal, Write, Written},
};
use crate::{
    errors::{ConstructionError, LifecycleError, database_error},
    runtime::{ConnectionState, NativeConnection, Operation},
};

pub(crate) fn state(
    py: Python<'_>,
    connection: &Bound<'_, PyAny>,
) -> PyResult<Arc<ConnectionState>> {
    let native = connection
        .getattr("_native")
        .unwrap_or_else(|_| connection.clone());
    let native = native
        .extract::<PyRef<'_, NativeConnection>>()
        .map_err(|_| {
            ConstructionError::new_err("entity execution requires an acquired pgorm Connection")
        })?;
    native.state.pool.check_owner(py)?;
    native.state.ensure_open()?;
    Ok(native.state.clone())
}

pub(crate) fn select<'py>(
    py: Python<'py>,
    connection: &Bound<'_, PyAny>,
    query: Select,
    terminal: Terminal,
) -> PyResult<Bound<'py, PyAny>> {
    let state = state(py, connection)?;
    if query.compile(terminal).1.0.len() > 65535 {
        return Err(ConstructionError::new_err(
            "PostgreSQL supports at most 65535 query parameters",
        ));
    }
    future_into_py(py, async move {
        let operation = Operation::begin(state.clone())?;
        let result = tokio::select! {
            _ = state.cancelled.cancelled() => return Err(LifecycleError::new_err("connection is closed")),
            _ = state.pool.cancelled.cancelled() => return Err(LifecycleError::new_err("pool is closed")),
            result = query.run(operation.connection()?, terminal) => result,
        };
        operation.restore();
        let models = result.map_err(|error| database_error(error, &state.pool.secrets))?;
        Python::attach(|py| {
            let models: Vec<_> = models
                .into_iter()
                .map(|inner| PyEntityModel { inner })
                .collect();
            for model in &models {
                model.validate(py)?;
            }
            if matches!(terminal, Terminal::All) {
                Ok(PyList::new(py, models)?.into_any().unbind())
            } else {
                match models.into_iter().next() {
                    Some(model) => model.into_py_any(py),
                    None => Ok(py.None()),
                }
            }
        })
    })
}

pub(crate) fn write<'py>(
    py: Python<'py>,
    connection: &Bound<'_, PyAny>,
    active: Active,
    write: Write,
) -> PyResult<Bound<'py, PyAny>> {
    let state = state(py, connection)?;
    future_into_py(py, async move {
        let operation = Operation::begin(state.clone())?;
        let result = tokio::select! {
            _ = state.cancelled.cancelled() => return Err(LifecycleError::new_err("connection is closed")),
            _ = state.pool.cancelled.cancelled() => return Err(LifecycleError::new_err("pool is closed")),
            result = active.run(operation.connection()?, write) => result,
        };
        operation.restore();
        let result = result.map_err(|error| database_error(error, &state.pool.secrets))?;
        Python::attach(|py| match result {
            Written::Count(count) => count.into_py_any(py),
            Written::Model(inner) => {
                let model = PyEntityModel { inner };
                model.validate(py)?;
                model.into_py_any(py)
            }
        })
    })
}
