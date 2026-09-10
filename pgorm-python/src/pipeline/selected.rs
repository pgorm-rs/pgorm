use super::{
    PyPipeline,
    selected_backend::{Factory, Info, Query, SourceFactory, Terminal},
    slots::SourceTypes,
};
use crate::{
    UnsupportedCapabilityError,
    entities::{NativeRegistry, Registry},
    errors::ConstructionError,
    expressions::Compiled,
    identifiers::validate_name,
};
use pgorm::pipeline::SourceList;
use pyo3::{prelude::*, types::PyString};
use std::{marker::PhantomData, sync::Arc};

impl Registry {
    /// Register a concrete tuple of one to six entity types for select_sources.
    /// Register each entity first; Python cannot invent Rust source types.
    // [spec:pgorm:req:python.pipeline]
    pub fn sources<T: SourceTypes>(&mut self, name: &str) -> PyResult<&mut Self>
    where
        <T::Selection as SourceList>::Row: Send,
    {
        if name.is_empty()
            || name.len() > 255
            || name.contains('\0')
            || self.sources.contains_key(name)
        {
            return Err(ConstructionError::new_err(
                "source registration requires a unique name of 1–255 UTF-8 bytes without NUL",
            ));
        }
        let info = Arc::new(Info {
            name: name.to_owned(),
            rust_shape: std::any::type_name::<T>(),
            bindings: T::bindings(self)?,
        });
        self.sources.insert(
            name.to_owned(),
            Arc::new(SourceFactory::<T> {
                info,
                marker: PhantomData,
            }),
        );
        Ok(self)
    }
}

#[derive(Debug, Clone)]
#[pyclass(
    name = "SourceSelection",
    module = "pgorm.pipeline",
    frozen,
    from_py_object
)]
pub struct PySourceSelection {
    pub(crate) factory: Arc<dyn Factory>,
}

#[pymethods]
impl PySourceSelection {
    #[getter]
    fn name(&self) -> &str {
        &self.factory.info().name
    }
    fn describe<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("json")?
            .call_method1("loads", (self.factory.info().describe().to_string(),))
    }
}

impl PySourceSelection {
    pub(super) fn select(
        &self,
        pipeline: &PyPipeline,
        qualifiers: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PySelectedSources> {
        let entities = &self.factory.info().bindings.entities;
        let qualifiers = match qualifiers.filter(|v| !v.is_none()) {
            Some(value) if !value.is_instance_of::<PyString>() => {
                value.extract::<Vec<String>>().map_err(|_| {
                    ConstructionError::new_err("qualifiers require a sequence of source names")
                })?
            }
            Some(_) => {
                return Err(ConstructionError::new_err(
                    "qualifiers require a sequence, not one string",
                ));
            }
            None => entities.iter().map(|e| e.table.clone()).collect(),
        };
        if qualifiers.len() != entities.len() {
            return Err(ConstructionError::new_err(
                "qualifier count must match the registered source count",
            ));
        }
        for qualifier in &qualifiers {
            validate_name(qualifier)?;
        }
        Ok(PySelectedSources {
            inner: self.factory.select(pipeline.inner.clone(), qualifiers),
        })
    }
}

#[derive(Debug, Clone)]
#[pyclass(
    name = "SelectedSources",
    module = "pgorm.pipeline",
    frozen,
    from_py_object
)]
pub struct PySelectedSources {
    inner: Query,
}

#[pymethods]
impl PySelectedSources {
    #[pyo3(signature=(*, terminal="all"))]
    fn inspect(&self, terminal: &str) -> PyResult<Compiled> {
        self.inner.compile(Terminal::parse(terminal)?)
    }
    fn all<'py>(
        &self,
        py: Python<'py>,
        connection: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        super::selected_io::read(py, connection, self.inner.clone(), Terminal::All)
    }
    fn one<'py>(
        &self,
        py: Python<'py>,
        connection: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        super::selected_io::read(py, connection, self.inner.clone(), Terminal::One)
    }
    fn one_opt<'py>(
        &self,
        py: Python<'py>,
        connection: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        super::selected_io::read(py, connection, self.inner.clone(), Terminal::Optional)
    }
    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "source selections cannot be tested as Python booleans",
        ))
    }
}

#[pyfunction(pass_module)]
pub(crate) fn pipeline_sources(
    module: &Bound<'_, PyModule>,
    name: &str,
) -> PyResult<PySourceSelection> {
    let object = module.getattr("_registry")?;
    let registry = object.extract::<PyRef<'_, NativeRegistry>>()?;
    registry
        .0
        .sources
        .get(name)
        .cloned()
        .map(|factory| PySourceSelection { factory })
        .ok_or_else(|| {
            UnsupportedCapabilityError::new_err(
                "source tuple is not registered in this native build",
            )
        })
}
