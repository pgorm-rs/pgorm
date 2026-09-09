//! Concrete Rust entity registrations, owned by one native extension module.

mod active;
pub(crate) mod backend;
mod column;
pub(crate) mod generic;
pub(crate) mod io;
pub(crate) mod metadata;
mod model;
pub(crate) mod query;
mod registry;

pub use active::{ActiveState, PyActiveModel, PyActiveValue};
pub use column::PyEntityColumn;
pub use model::PyEntityModel;
pub use query::PyEntityQuery;
pub(crate) use registry::NativeRegistry;
pub use registry::Registry;

use crate::identifiers::PyIdentifier;
use pyo3::prelude::*;
use std::sync::Arc;

#[derive(Clone, Debug)]
#[pyclass(name = "Entity", module = "pgorm", frozen, from_py_object)]
pub struct PyEntity {
    pub(crate) backend: Arc<dyn backend::EntityBackend>,
}

#[pymethods]
impl PyEntity {
    #[getter]
    fn name(&self) -> &str {
        &self.backend.info().name
    }

    fn describe<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("json")?
            .call_method1("loads", (self.backend.info().describe().to_string(),))
    }

    fn col(&self, name: &Bound<'_, PyAny>) -> PyResult<PyEntityColumn> {
        let name = PyIdentifier::new(name)?.name;
        self.backend.info().column(&name)?;
        Ok(PyEntityColumn {
            entity: self.backend.clone(),
            name,
        })
    }

    fn find(&self) -> PyEntityQuery {
        PyEntityQuery {
            inner: self.backend.select(),
        }
    }

    fn active(&self) -> PyActiveModel {
        PyActiveModel {
            inner: self.backend.active(),
        }
    }

    fn __repr__(&self) -> String {
        format!("Entity({:?})", self.name())
    }
}

pub(crate) fn install(module: &Bound<'_, PyModule>, registry: Registry) -> PyResult<()> {
    module.add_class::<PyEntity>()?;
    module.add_class::<PyEntityColumn>()?;
    module.add_class::<PyEntityQuery>()?;
    module.add_class::<PyEntityModel>()?;
    module.add_class::<PyActiveModel>()?;
    module.add_class::<PyActiveValue>()?;
    module.add_class::<ActiveState>()?;
    module.add("_registry", registry::NativeRegistry(registry))?;
    module.add_function(wrap_pyfunction!(registry::entity, module)?)?;
    Ok(())
}

pub(crate) fn capabilities() -> serde_json::Map<String, serde_json::Value> {
    [
        ("entity", "pgorm_python::entities::Registry"),
        ("entity.find", "pgorm::EntityTrait::find"),
        ("entity.column", "pgorm::ColumnTrait"),
        ("entity.all", "pgorm::Select<E>::all"),
        ("entity.one", "pgorm::Select<E>::one"),
        ("entity.one_opt", "pgorm::Select<E>::one_opt"),
        ("entity.model", "pgorm::ModelTrait"),
        (
            "entity.active",
            "pgorm::ActiveModelBehavior::new / IntoActiveModel",
        ),
        ("entity.active.insert", "pgorm::ActiveModelTrait::insert"),
        ("entity.active.update", "pgorm::ActiveModelTrait::update"),
        ("entity.active.delete", "pgorm::ActiveModelTrait::delete"),
    ]
    .into_iter()
    .map(|(name, api)| {
        (
            name.to_owned(),
            serde_json::json!({"rust_api": api, "features": [], "registration_required": true}),
        )
    })
    .collect()
}
