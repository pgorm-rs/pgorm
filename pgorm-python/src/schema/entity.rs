use super::{PyCreateIndex, PyCreateTable, PyDDL};
use crate::{UnsupportedCapabilityError, entities::PyEntity, errors::ConstructionError};
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

#[pymethods]
impl PyEntitySchema {
    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "entity schemas cannot be tested as Python booleans",
        ))
    }
}

// [spec:pgorm:req:bindings.schema-from-entity]
#[pyfunction]
pub(super) fn schema_from_entity(entity: &Bound<'_, PyAny>) -> PyResult<PyEntitySchema> {
    let entity = entity.extract::<PyRef<'_, PyEntity>>().map_err(|_| {
        UnsupportedCapabilityError::new_err("schema.from_entity requires a registered native Entity; runtime models use explicit DDL builders")
    })?;
    entity
        .backend
        .schema()
        .map_err(|error| ConstructionError::new_err(error.to_string()))
}
