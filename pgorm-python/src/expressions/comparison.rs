//! `Expr`'s comparisons: the six ordering operators, as methods and as
//! Python's rich comparisons, each taking `any_(..)` or `all_(..)` as well as
//! an operand, and the null-safe `IS [NOT] DISTINCT FROM`.

use pgorm::pgorm_query::{BinOper, Expr};
use pyo3::{basic::CompareOp, prelude::*};

use super::{PyExpr, coerce};
use crate::statements::PyQuantified;

impl PyExpr {
    /// `operator` with `other`, which may be `ANY (..)` or `ALL (..)`: a
    /// comparison is the one operator PostgreSQL's grammar lets one follow.
    fn compare(&self, operator: BinOper, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        match other.extract::<PyRef<'_, PyQuantified>>() {
            Ok(quantified) => Ok(Self::from_rust(
                self.inner
                    .clone()
                    .binary(operator, quantified.inner.clone()),
            )),
            Err(_) => self.binary(operator, other),
        }
    }
}

// [spec:pgorm:req:bindings.expressions+2]
// [spec:pgorm:req:python.naming+2]
#[pymethods]
impl PyExpr {
    fn eq(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.compare(BinOper::Equal, other)
    }
    fn ne(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.compare(BinOper::NotEqual, other)
    }
    fn gt(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.compare(BinOper::GreaterThan, other)
    }
    fn gte(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.compare(BinOper::GreaterThanOrEqual, other)
    }
    fn lt(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.compare(BinOper::SmallerThan, other)
    }
    fn lte(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.compare(BinOper::SmallerThanOrEqual, other)
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, operator: CompareOp) -> PyResult<Self> {
        self.compare(
            match operator {
                CompareOp::Eq => BinOper::Equal,
                CompareOp::Ne => BinOper::NotEqual,
                CompareOp::Lt => BinOper::SmallerThan,
                CompareOp::Le => BinOper::SmallerThanOrEqual,
                CompareOp::Gt => BinOper::GreaterThan,
                CompareOp::Ge => BinOper::GreaterThanOrEqual,
            },
            other,
        )
    }

    /// `IS DISTINCT FROM`: inequality that reads NULL as a value, so it
    /// answers true or false for every pair.
    fn is_distinct_from(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(
            Expr::expr(self.inner.clone()).is_distinct_from(coerce(other)?.inner),
        ))
    }
    fn is_not_distinct_from(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(
            Expr::expr(self.inner.clone()).is_not_distinct_from(coerce(other)?.inner),
        ))
    }
}
