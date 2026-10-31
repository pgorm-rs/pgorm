use pgorm::pgorm_query::LikeExpr;
use pyo3::{exceptions::PyTypeError, prelude::*};

use super::{PyExpr, construct::require_expr};
use crate::{
    errors::ConstructionError,
    identifiers::{Direction, Nulls, PyIdentifier},
};

/// An explicit SQL LIKE pattern, including its optional escape character; an
/// empty escape turns escaping off.
#[pyclass(name = "LikePattern", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct LikePattern {
    pub inner: LikeExpr,
}

#[pymethods]
impl LikePattern {
    #[new]
    #[pyo3(signature = (pattern, *, escape=None))]
    fn new(pattern: &Bound<'_, PyAny>, escape: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let pattern: String = pattern
            .extract()
            .map_err(|_| PyTypeError::new_err("LIKE pattern requires a string"))?;
        let mut inner = LikeExpr::new(pattern);
        if let Some(escape) = escape {
            let refused = || {
                ConstructionError::new_err(
                    "LIKE escape requires one Unicode scalar, or an empty string to turn escaping off",
                )
            };
            let escape: String = escape
                .extract()
                .map_err(|_| PyTypeError::new_err("LIKE escape requires a string"))?;
            let mut chars = escape.chars();
            inner = match (chars.next(), chars.next()) {
                (None, _) => inner.no_escape(),
                (Some('\0'), None) => {
                    return Err(ConstructionError::new_err("LIKE escape cannot contain NUL"));
                }
                (Some(character), None) => inner.escape(character),
                (Some(_), Some(_)) => return Err(refused()),
            };
        }
        Ok(Self { inner })
    }

    fn __repr__(&self) -> &'static str {
        "LikePattern(...)"
    }

    // [spec:pgorm:req:python.truth-testing+3]
    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "LIKE patterns cannot be tested as Python booleans",
        ))
    }
}

/// A projection expression with a separate, validated output identifier.
#[pyclass(name = "Aliased", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct Aliased {
    #[pyo3(get)]
    pub expr: PyExpr,
    #[pyo3(get)]
    pub alias: PyIdentifier,
}

#[pymethods]
impl Aliased {
    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "SQL projections cannot be tested as Python booleans",
        ))
    }
}

/// An expression and typed direction/NULL placement for a statement's ORDER BY.
#[pyclass(name = "OrderBy", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct OrderBy {
    #[pyo3(get)]
    pub expr: PyExpr,
    #[pyo3(get)]
    pub direction: Direction,
    #[pyo3(get)]
    pub nulls: Option<Nulls>,
}

#[pymethods]
impl OrderBy {
    #[new]
    #[pyo3(signature = (expr, direction, *, nulls=None))]
    fn new(expr: &Bound<'_, PyAny>, direction: Direction, nulls: Option<Nulls>) -> PyResult<Self> {
        Ok(Self {
            expr: require_expr(expr)?,
            direction,
            nulls,
        })
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "SQL orderings cannot be tested as Python booleans",
        ))
    }
}

pub(super) fn pattern(value: &Bound<'_, PyAny>) -> PyResult<LikeExpr> {
    value
        .extract::<PyRef<'_, LikePattern>>()
        .map(|value| value.inner.clone())
        .map_err(|_| PyTypeError::new_err("use LikePattern to select SQL pattern semantics"))
}
