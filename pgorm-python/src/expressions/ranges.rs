//! `Expr`'s containment and position operators: `@>`, `<@` and `&&` over
//! ranges, multiranges and arrays, and where one range or multirange lies
//! against another.

use pgorm::pgorm_query::BinOper;
use pyo3::prelude::*;

use super::PyExpr;

// [spec:pgorm:req:python.expressions+4]
#[pymethods]
impl PyExpr {
    fn contains(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::Contains, other)
    }
    fn contained_by(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::Contained, other)
    }
    fn overlaps(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::Overlap, other)
    }
    /// `<<`: lying wholly left of the other. The operator is the shift's,
    /// so it is Rust's `LShift`.
    fn strictly_left_of(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::LShift, other)
    }
    fn strictly_right_of(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::RShift, other)
    }
    fn not_extend_right_of(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::NotExtendRightOf, other)
    }
    fn not_extend_left_of(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::NotExtendLeftOf, other)
    }
    fn adjacent_to(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::AdjacentTo, other)
    }
}
