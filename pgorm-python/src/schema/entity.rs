use super::{PyCreateIndex, PyCreateTable, PyDDL};
use crate::{UnsupportedCapabilityError, entities::PyEntity};
use pyo3::prelude::*;

/// A detached result of Rust's Schema methods for a compiled entity.
#[derive(Clone, Debug)]
#[pyclass(name = "EntitySchema", module = "pgorm.schema", frozen, from_py_object)]
pub struct PyEntitySchema {
    #[pyo3(get)]
    pub(crate) table: PyCreateTable,
    #[pyo3(get)]
    pub(crate) enums: Vec<PyDDL>,
    #[pyo3(get)]
    pub(crate) indexes: Vec<PyCreateIndex>,
    #[pyo3(get)]
    pub(crate) comments: Vec<PyDDL>,
}

// [spec:pgorm:req:python.schema]
#[pyfunction]
pub(super) fn schema_from_entity(entity: &Bound<'_, PyAny>) -> PyResult<PyEntitySchema> {
    let entity = entity.extract::<PyRef<'_, PyEntity>>().map_err(|_| {
        UnsupportedCapabilityError::new_err("schema.from_entity requires a registered native Entity; runtime models use explicit DDL builders")
    })?;
    Ok(entity.backend.schema())
}
