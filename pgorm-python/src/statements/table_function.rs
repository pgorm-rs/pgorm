//! A set-returning function read as a FROM item, over Rust's
//! `TableFunction`.

use pgorm::pgorm_query::{FunctionCall, SimpleExpr, TableFunction};
use pyo3::{prelude::*, types::PyTuple};

use super::from_item::PyFromItem;
use crate::{errors::ConstructionError, expressions::PyExpr, identifiers::PyIdentifier};

/// The call an expression holds; anything else cannot stand in FROM.
fn function_call(value: &Bound<'_, PyAny>) -> PyResult<FunctionCall> {
    let expr = value.extract::<PyRef<'_, PyExpr>>().ok();
    match expr.as_deref().map(|expr| &expr.inner) {
        Some(SimpleExpr::FunctionCall(call)) => Ok(call.clone()),
        _ => Err(ConstructionError::new_err(
            "a table function requires a function call built by call()",
        )),
    }
}

// [spec:pgorm:req:python.statements+7]
/// A function in FROM: one call, or several read side by side under
/// `ROWS FROM (..)`, the shorter padded with NULLs; `with_ordinality` adds a
/// final `bigint` column numbering the rows from 1, and `columns` names the
/// columns in the alias. `alias` finishes it as the `FromItem` that `from_`
/// and the joins take.
#[pyclass(name = "TableFunction", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyTableFunction {
    inner: TableFunction,
}

#[pymethods]
impl PyTableFunction {
    #[new]
    fn new(call: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: TableFunction::new(function_call(call)?),
        })
    }

    /// `ROWS FROM (f(..))`, to which `and_` adds calls. Over one call it
    /// differs from the constructor only for a function returning a
    /// composite, whose columns `ROWS FROM` expands.
    #[staticmethod]
    fn rows_from(call: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: TableFunction::rows_from(function_call(call)?),
        })
    }

    /// Read another call beside the others; several calls are `ROWS FROM`.
    fn and_(&self, call: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.clone().and(function_call(call)?),
        })
    }

    fn with_ordinality(&self) -> Self {
        Self {
            inner: self.inner.clone().with_ordinality(),
        }
    }

    /// Name the relation's columns in its alias, the ordinal column last,
    /// replacing any names given before.
    #[pyo3(signature = (*names))]
    fn columns(&self, names: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let names = names
            .iter()
            .map(|name| PyIdentifier::new(&name).map(|name| name.name()))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            inner: self.inner.clone().columns(names),
        })
    }

    fn alias(&self, name: &Bound<'_, PyAny>) -> PyResult<PyFromItem> {
        Ok(PyFromItem {
            inner: self.inner.clone().alias(PyIdentifier::new(name)?.name()),
        })
    }

    fn __repr__(&self) -> &'static str {
        "TableFunction(...)"
    }
}
