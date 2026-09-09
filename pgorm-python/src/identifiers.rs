use pgorm::pgorm_query::{Alias, NullOrdering, Order};
use pyo3::prelude::*;

use crate::errors::ConstructionError;

// [spec:pgorm:req:python.input-boundaries]
/// One owned PostgreSQL identifier; punctuation is never parsed as SQL.
#[pyclass(name = "Identifier", module = "pgorm", frozen, eq, from_py_object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyIdentifier {
    #[pyo3(get)]
    pub name: String,
}

#[pymethods]
impl PyIdentifier {
    #[new]
    pub fn new(name: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(name) = name.extract::<PyRef<'_, Self>>() {
            return Ok(name.clone());
        }
        let name = name
            .extract::<String>()
            .map_err(|_| ConstructionError::new_err("identifier requires a string"))?;
        validate_name(&name)?;
        Ok(Self { name })
    }

    fn __repr__(&self) -> String {
        format!("Identifier({:?})", self.name)
    }
}

impl PyIdentifier {
    pub fn alias(&self) -> Alias {
        Alias::new(&self.name)
    }
}

pub(crate) fn validate_name(name: &str) -> PyResult<()> {
    if name.is_empty() || name.contains('\0') || name.len() > 63 {
        Err(ConstructionError::new_err(
            "identifier parts require 1–63 UTF-8 bytes without NUL",
        ))
    } else {
        Ok(())
    }
}

/// Typed SQL ordering direction.
#[pyclass(name = "Direction", module = "pgorm", eq, from_py_object)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Asc,
    Desc,
}

impl Direction {
    pub fn rust_order(self) -> Order {
        match self {
            Self::Asc => Order::Asc,
            Self::Desc => Order::Desc,
        }
    }
}

/// Typed placement of NULL values in an ordering.
#[pyclass(name = "Nulls", module = "pgorm", eq, from_py_object)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nulls {
    First,
    Last,
}

impl Nulls {
    pub fn rust_nulls(self) -> NullOrdering {
        match self {
            Self::First => NullOrdering::First,
            Self::Last => NullOrdering::Last,
        }
    }
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyIdentifier>()?;
    module.add_class::<Direction>()?;
    module.add_class::<Nulls>()?;
    Ok(())
}
