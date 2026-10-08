//! Owned Python values backed by pgorm's tagged Rust Value representation.

mod classes;
mod convert;
mod created;
mod json;
mod ranges;
mod snapshot;
mod temporal;
mod types;

use pgorm::pgorm_query::{Expr, SimpleExpr, Value};
use pyo3::{prelude::*, types::PyList};

use crate::errors::ConstructionError;
pub use classes::{PyMultirange, PyRange};
pub(crate) use created::{CreatedKind, extract as created_kind};
pub use created::{PyCreatedMultirange, PyCreatedRange};
pub(crate) use ranges::{column_type as range_column_type, kind_names as range_kind_names};
pub use types::PyTypeName;
use types::Tag;
pub(crate) use types::{SCALAR_NAMES, scalar_name};

// [spec:pgorm:req:python.values+2]
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
    pub fn from_rust(inner: Value) -> PyResult<Self> {
        Ok(Self {
            tag: types::rust_tag(&inner),
            inner,
        })
    }

    /// Borrow the value for Rust builder and parameter APIs.
    pub fn rust_value(&self) -> &Value {
        &self.inner
    }

    /// Retain a PostgreSQL enum's qualified identity alongside its Rust payload.
    pub(crate) fn from_enum(inner: Value, name: PyTypeName, array: bool) -> Self {
        let tag = Tag::Enum(name);
        Self {
            inner,
            tag: if array {
                Tag::Array(Box::new(tag))
            } else {
                tag
            },
        }
    }

    /// Convert through the same checked path used by the public Value getter.
    /// A created range's text is read back as the range it spells.
    pub(crate) fn to_python(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match (&self.tag, &self.inner) {
            (Tag::Created(kind), Value::String(Some(text))) => kind.read(py, text),
            _ => convert::to_python(py, &self.inner),
        }
    }

    /// A created range or multirange's value: its text form, or SQL NULL.
    pub(crate) fn from_created(text: Option<String>, kind: CreatedKind) -> Self {
        Self {
            inner: Value::String(text.map(Box::new)),
            tag: Tag::Created(kind),
        }
    }

    /// The created range or multirange type this value belongs to, if any.
    pub(crate) fn created(&self) -> Option<&CreatedKind> {
        match &self.tag {
            Tag::Created(kind) => Some(kind),
            _ => None,
        }
    }

    /// Whether this value's SQL type is one the server names: a qualified enum
    /// or a created range, which plain binding cannot spell.
    pub(crate) fn has_named_type(&self) -> bool {
        self.enum_cast().is_some() || self.created().is_some()
    }

    /// `expression`, which holds this value, written as its type needs: an
    /// enum label cast to its enum, a created range's text cast to its range
    /// through `Expr::as_range`, anything else as it is.
    pub(crate) fn typed(&self, expression: SimpleExpr) -> SimpleExpr {
        if let Some(cast) = self.enum_cast() {
            expression.cast_as_type(cast)
        } else if let Some(kind) = self.created() {
            Expr::expr(expression).as_range(kind.name.rust_type())
        } else {
            expression
        }
    }

    /// The qualified enum cast carried by this scalar or array, if any.
    pub fn enum_cast(&self) -> Option<pgorm::pgorm_query::TypeName> {
        match &self.tag {
            Tag::Enum(name) => Some(name.rust_type()),
            Tag::Array(element) => match element.as_ref() {
                Tag::Enum(name) => Some(name.rust_type().array()),
                _ => None,
            },
            _ => None,
        }
    }

    pub(crate) fn coerce(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Self::new(value, None)
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
        } else if let Tag::Created(created) = &tag {
            Value::String(Some(Box::new(created.text(data)?)))
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
        Self::from_rust(Value::Json(Some(Box::new(
            json::from_python(value, 0).map_err(construction_error)?,
        ))))
    }

    #[staticmethod]
    fn array(kind: &Bound<'_, PyAny>, values: &Bound<'_, PyAny>) -> PyResult<Self> {
        let result = (|| {
            let element = Tag::parse(kind)?;
            if matches!(element, Tag::Created(_)) {
                return Err(ConstructionError::new_err(
                    "arrays of a created range or multirange type are not supported",
                ));
            }
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
            Tag::Created(kind) => Some(kind.name.clone()),
            _ => None,
        }
    }

    /// The `CreatedRange` or `CreatedMultirange` kind of a created range's
    /// value.
    #[getter]
    fn created_type(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self.created().map(|kind| kind.to_python(py)).transpose()
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
        self.to_python(py)
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
        json::to_python(py, &snapshot::encode(self)?)
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
    module.add_class::<PyRange>()?;
    module.add_class::<PyMultirange>()?;
    module.add_class::<PyCreatedRange>()?;
    module.add_class::<PyCreatedMultirange>()?;
    Ok(())
}
