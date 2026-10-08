//! Common table expressions, and the two versions of a written row a
//! RETURNING list reads.

use pgorm::pgorm_query::{Asterisk, CommonTableExpression, Expr, ReturningRow, WithClause};
use pyo3::{exceptions::PyTypeError, prelude::*};

use super::{merge::PyMerge, select::PySelect};
use crate::{expressions::PyExpr, identifiers::PyIdentifier};

/// One common table expression. Its body is a SELECT, or a MERGE, whose rows
/// are those its RETURNING list yields.
fn expression(
    name: &Bound<'_, PyAny>,
    query: &Bound<'_, PyAny>,
) -> PyResult<CommonTableExpression> {
    let name = PyIdentifier::new(name)?.name();
    if let Ok(select) = query.extract::<PyRef<'_, PySelect>>() {
        Ok(CommonTableExpression::new(name, select.inner.clone()))
    } else if let Ok(merge) = query.extract::<PyRef<'_, PyMerge>>() {
        Ok(CommonTableExpression::new(name, merge.inner.clone()))
    } else {
        Err(PyTypeError::new_err(
            "a common table expression's body is a Select or a Merge",
        ))
    }
}

/// `WITH name AS (query), ..`: one common table expression or more, in order.
#[pyclass(name = "With", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyWith {
    pub(super) inner: WithClause,
}

#[pymethods]
impl PyWith {
    #[new]
    fn new(name: &Bound<'_, PyAny>, query: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: WithClause::new(expression(name, query)?),
        })
    }

    fn cte(&self, name: &Bound<'_, PyAny>, query: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut inner = self.inner.clone();
        inner.cte(expression(name, query)?);
        Ok(Self { inner })
    }

    fn __repr__(&self) -> String {
        format!("With(ctes={})", self.inner.ctes().count())
    }
}

/// The row as it was before the write (`Old`) or as the statement left it
/// (`New`), read in a RETURNING list. With `returning(old_as=..)` or
/// `new_as=..` the version answers only to its new name, read as a table.
#[pyclass(name = "ReturningRow", module = "pgorm", eq, from_py_object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PyReturningRow {
    Old,
    New,
}

impl PyReturningRow {
    fn rust(&self) -> ReturningRow {
        match self {
            Self::Old => ReturningRow::Old,
            Self::New => ReturningRow::New,
        }
    }
}

#[pymethods]
impl PyReturningRow {
    fn col(&self, name: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
        let name = PyIdentifier::new(name)?.name();
        Ok(PyExpr::from_rust(Expr::col((self.rust(), name)).into()))
    }

    fn star(&self) -> PyExpr {
        PyExpr::from_rust(Expr::col((self.rust(), Asterisk)).into())
    }
}
