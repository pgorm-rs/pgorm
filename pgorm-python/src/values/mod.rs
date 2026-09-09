//! Owned Python values backed by pgorm's tagged Rust Value representation.

mod convert;
mod json;
mod snapshot;
mod temporal;
mod types;

use pgorm::pgorm_query::Value;
use pyo3::{prelude::*, types::PyList};

use crate::errors::ConstructionError;
pub use types::PyTypeName;
pub(crate) use types::SCALAR_NAMES;
use types::Tag;

// [spec:pgorm:req:python.values]
// [spec:pgorm:req:python.value-tags]
/// Immutable owned Rust value plus qualified enum/array identity.
#[pyclass(name = "Value", module = "pgorm", frozen, eq, from_py_object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyValue {
    pub(crate) inner: Value,
    tag: Tag,
}

impl PyValue {
    /// Wrap a Rust value without changing its variant or payload.
    pub fn from_rust(inner: Value) -> Self {
        Self {
            tag: types::rust_tag(&inner),
            inner,
        }
    }

    /// Borrow the value for Rust builder and parameter APIs.
    pub fn rust_value(&self) -> &Value {
        &self.inner
    }

    fn convert(data: &Bound<'_, PyAny>, tag: Tag) -> PyResult<Self> {
        if let Ok(value) = data.extract::<PyRef<'_, Self>>() {
            if value.tag != tag {
                return Err(ConstructionError::new_err(
                    "tagged value has an incompatible type",
                ));
            }
            return Ok(value.clone());
        }
        let kind = tag.array_type()?;
        let inner = if data.is_none() {
            types::scalar_null(&kind)
        } else {
            convert::from_python(data, &kind)?
        };
        Ok(Self { inner, tag })
    }
}

#[pymethods]
impl PyValue {
    #[new]
    #[pyo3(signature = (value, kind=None))]
    fn new(value: &Bound<'_, PyAny>, kind: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let result = (|| {
            if kind.is_none()
                && let Ok(existing) = value.extract::<PyRef<'_, Self>>()
            {
                return Ok(existing.clone());
            }
            let tag = match kind {
                Some(kind) => Tag::parse(kind)?,
                None => Tag::Scalar(convert::infer(value)?),
            };
            Self::convert(value, tag)
        })();
        result.map_err(construction_error)
    }

    #[staticmethod]
    fn null(kind: &Bound<'_, PyAny>) -> PyResult<Self> {
        let tag = Tag::parse(kind).map_err(construction_error)?;
        Ok(Self {
            inner: types::scalar_null(&tag.array_type()?),
            tag,
        })
    }

    #[staticmethod]
    fn json(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(Value::Json(Some(Box::new(
            json::from_python(value, 0).map_err(construction_error)?,
        )))))
    }

    #[staticmethod]
    fn array(kind: &Bound<'_, PyAny>, values: &Bound<'_, PyAny>) -> PyResult<Self> {
        let result = (|| {
            let element = Tag::parse(kind)?;
            let array_type = element.array_type()?;
            let inner = if values.is_none() {
                None
            } else {
                convert::require_sequence(values)?;
                let items = values
                    .try_iter()?
                    .map(|item| Self::convert(&item?, element.clone()).map(|value| value.inner))
                    .collect::<PyResult<Vec<_>>>()?;
                Some(Box::new(items))
            };
            Ok(Self {
                inner: Value::Array(array_type, inner),
                tag: Tag::Array(Box::new(element)),
            })
        })();
        result.map_err(construction_error)
    }

    #[getter]
    fn kind(&self) -> &'static str {
        self.tag.name()
    }

    #[getter]
    fn is_null(&self) -> bool {
        types::is_null(&self.inner)
    }

    #[getter]
    fn type_name(&self) -> Option<PyTypeName> {
        match &self.tag {
            Tag::Enum(name) => Some(name.clone()),
            _ => None,
        }
    }

    #[getter]
    fn element_type(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        match &self.tag {
            Tag::Array(element) => Ok(Some(element.to_python(py)?)),
            _ => Ok(None),
        }
    }

    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        convert::to_python(py, &self.inner)
    }

    /// Obtain independently owned, tagged array elements (None for an SQL NULL array).
    fn items(&self, py: Python<'_>) -> PyResult<Option<Py<PyList>>> {
        match (&self.tag, &self.inner) {
            (Tag::Array(tag), Value::Array(_, values)) => values
                .as_ref()
                .map(|values| {
                    let values = values.iter().map(|inner| Self {
                        inner: inner.clone(),
                        tag: (**tag).clone(),
                    });
                    Ok(PyList::new(py, values)?.unbind())
                })
                .transpose(),
            _ => Err(ConstructionError::new_err("items requires an array value")),
        }
    }

    /// JSON-compatible, lossless inspection data. Float payloads use IEEE bits.
    fn snapshot(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        json::to_python(py, &snapshot::encode(self))
    }

    fn __repr__(&self) -> String {
        format!("Value(kind={:?}, is_null={})", self.kind(), self.is_null())
    }
}

fn construction_error(error: PyErr) -> PyErr {
    Python::attach(|py| {
        if error.is_instance_of::<ConstructionError>(py) {
            error
        } else {
            ConstructionError::new_err(error.to_string())
        }
    })
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyValue>()?;
    module.add_class::<PyTypeName>()?;
    Ok(())
}
