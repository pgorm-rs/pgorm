//! Expressions over a subquery — `ARRAY (..)`, `EXISTS (..)` and a scalar
//! subquery — and the quantifiers `ANY (..)` and `ALL (..)`, over a
//! subquery's rows or an array.

use pgorm::pgorm_query::{Expr, Func, SelectStatement, SimpleExpr, SubQueryStatement};
use pyo3::prelude::*;

use super::select::PySelect;
use crate::{
    errors::ConstructionError,
    expressions::{PyExpr, coerce},
};

/// The statement `value` is when it is a `Select`, read as any embedded one
/// is, so it is checked as `PySelect::embedded` checks it.
pub(crate) fn subquery(value: &Bound<'_, PyAny>) -> PyResult<Option<SelectStatement>> {
    match value.extract::<PyRef<'_, PySelect>>() {
        Ok(select) => select.embedded().map(Some),
        Err(_) => Ok(None),
    }
}

/// `array(select)`: `ARRAY(select)`, the subquery's one column collected into
/// one array value through Rust's `Expr::array`, which can be cast and
/// compared as a value where a subquery's rows cannot. The subquery is read
/// as any embedded one is, so it is checked as `embedded` checks it.
// [spec:pgorm:req:bindings.expressions+2]
#[pyfunction(name = "array")]
pub(crate) fn subquery_array(select: PyRef<'_, PySelect>) -> PyResult<PyExpr> {
    Ok(PyExpr::from_rust(Expr::array(select.embedded()?)))
}

/// `exists(select)`: `EXISTS (select)` through Rust's `Expr::exists`,
/// whether the subquery yields a row.
// [spec:pgorm:req:bindings.expressions+2]
#[pyfunction(name = "exists")]
pub(crate) fn subquery_exists(select: PyRef<'_, PySelect>) -> PyResult<PyExpr> {
    Ok(PyExpr::from_rust(Expr::exists(select.embedded()?)))
}

/// `scalar(select)`: `(select)`, a subquery read as the one value of its one
/// row, NULL when it yields none. PostgreSQL refuses one of more than one
/// column (`42601`) and, as it runs, one yielding more than one row (`21000`).
// [spec:pgorm:req:bindings.expressions+2]
#[pyfunction(name = "scalar")]
pub(crate) fn subquery_scalar(select: PyRef<'_, PySelect>) -> PyResult<PyExpr> {
    let select = SubQueryStatement::SelectStatement(select.embedded()?);
    Ok(PyExpr::from_rust(SimpleExpr::SubQuery(
        None,
        Box::new(select),
    )))
}

/// `ANY (..)` or `ALL (..)`: the right operand of a comparison and nothing
/// else, the one place PostgreSQL's grammar reads it, so no other operator
/// takes it and it has no SQL of its own to inspect.
// [spec:pgorm:req:bindings.quantifiers]
#[pyclass(name = "Quantified", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyQuantified {
    pub(crate) inner: SimpleExpr,
}

#[pymethods]
impl PyQuantified {
    // [spec:pgorm:req:python.truth-testing+3]
    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "ANY and ALL cannot be tested as Python booleans",
        ))
    }
    fn __repr__(&self) -> &'static str {
        "Quantified(...)"
    }
}

/// `any_(source)`: `ANY (..)` over a `Select`'s rows, Rust's `Expr::any`, or
/// over an array, Rust's `Func::any`, the array bound unless it is an
/// expression. The comparison holds for some element or row; over none it is
/// false.
// [spec:pgorm:req:bindings.quantifiers]
#[pyfunction(name = "any_")]
pub(crate) fn quantified_any(source: &Bound<'_, PyAny>) -> PyResult<PyQuantified> {
    let inner = match subquery(source)? {
        Some(select) => Expr::any(select),
        None => Func::any(coerce(source)?.inner).into(),
    };
    Ok(PyQuantified { inner })
}

/// `all_(source)`: `ALL (..)` over a `Select`'s rows, Rust's `Expr::all`, or
/// over an array, Rust's `Func::all`. The comparison holds for every element
/// or row; over none it is true.
// [spec:pgorm:req:bindings.quantifiers]
#[pyfunction(name = "all_")]
pub(crate) fn quantified_all(source: &Bound<'_, PyAny>) -> PyResult<PyQuantified> {
    let inner = match subquery(source)? {
        Some(select) => Expr::all(select),
        None => Func::all(coerce(source)?.inner).into(),
    };
    Ok(PyQuantified { inner })
}
