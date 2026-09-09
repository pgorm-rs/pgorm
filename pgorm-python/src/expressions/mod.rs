//! Composable, owned wrappers of the existing Rust expression builders.

mod capabilities;
mod compiled;
mod construct;
mod options;
#[cfg(test)]
mod tests;

use pgorm::pgorm_query::{Alias, BinOper, Expr, Func, Query, SimpleExpr};
use pyo3::{basic::CompareOp, prelude::*};

use crate::{
    errors::ConstructionError,
    identifiers::{Direction, Nulls, PyIdentifier},
    values::PyTypeName,
};
pub(crate) use capabilities::operations as capabilities;
pub use compiled::Compiled;
pub use construct::PyCondition;
use construct::sequence;
pub(crate) use construct::{coerce, require_expr};
pub use options::{AliasedExpr, LikePattern, OrderBy};

// [spec:pgorm:req:python.expressions]
// [spec:pgorm:req:python.ownership]
/// An immutable owned Rust SimpleExpr. Every composition clones its inputs.
#[pyclass(name = "Expr", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyExpr {
    pub inner: SimpleExpr,
}

impl PyExpr {
    pub fn from_rust(inner: SimpleExpr) -> Self {
        Self { inner }
    }

    fn binary(&self, operator: BinOper, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(
            self.inner.clone().binary(operator, coerce(other)?.inner),
        ))
    }
}

#[pymethods]
impl PyExpr {
    fn eq(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::Equal, other)
    }
    fn ne(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::NotEqual, other)
    }
    fn gt(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::GreaterThan, other)
    }
    fn gte(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::GreaterThanOrEqual, other)
    }
    fn lt(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::SmallerThan, other)
    }
    fn lte(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::SmallerThanOrEqual, other)
    }

    fn __richcmp__(&self, other: &Bound<'_, PyAny>, operator: CompareOp) -> PyResult<Self> {
        self.binary(
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

    fn __add__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::Add, other)
    }
    fn __sub__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::Sub, other)
    }
    fn __mul__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::Mul, other)
    }
    fn __truediv__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::Div, other)
    }
    fn __mod__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.binary(BinOper::Mod, other)
    }
    fn __and__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(
            self.inner
                .clone()
                .and(construct::require_expr(other)?.inner),
        ))
    }
    fn __or__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(
            self.inner.clone().or(construct::require_expr(other)?.inner),
        ))
    }
    fn __invert__(&self) -> Self {
        Self::from_rust(self.inner.clone().not())
    }
    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "SQL expressions cannot be tested as Python booleans; use &, | and ~",
        ))
    }

    fn is_null(&self) -> Self {
        Self::from_rust(Expr::expr(self.inner.clone()).is_null())
    }
    fn is_not_null(&self) -> Self {
        Self::from_rust(Expr::expr(self.inner.clone()).is_not_null())
    }

    fn is_in(&self, values: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(
            Expr::expr(self.inner.clone()).is_in(sequence(values)?),
        ))
    }
    fn is_not_in(&self, values: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(
            Expr::expr(self.inner.clone()).is_not_in(sequence(values)?),
        ))
    }
    fn between(&self, lower: &Bound<'_, PyAny>, upper: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(
            Expr::expr(self.inner.clone()).between(coerce(lower)?.inner, coerce(upper)?.inner),
        ))
    }
    fn not_between(&self, lower: &Bound<'_, PyAny>, upper: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(
            Expr::expr(self.inner.clone()).not_between(coerce(lower)?.inner, coerce(upper)?.inner),
        ))
    }

    #[pyo3(signature = (type_name, *, array=false))]
    fn cast(&self, type_name: PyRef<'_, PyTypeName>, array: bool) -> Self {
        let name = type_name.rust_type();
        Self::from_rust(
            self.inner
                .clone()
                .cast_as_type(if array { name.array() } else { name }),
        )
    }
    fn as_(&self, alias: &Bound<'_, PyAny>) -> PyResult<AliasedExpr> {
        Ok(AliasedExpr {
            expr: self.clone(),
            alias: PyIdentifier::new(alias)?,
        })
    }
    #[pyo3(signature = (*, nulls=None))]
    fn asc(&self, nulls: Option<Nulls>) -> OrderBy {
        OrderBy {
            expr: self.clone(),
            direction: Direction::Asc,
            nulls,
        }
    }
    #[pyo3(signature = (*, nulls=None))]
    fn desc(&self, nulls: Option<Nulls>) -> OrderBy {
        OrderBy {
            expr: self.clone(),
            direction: Direction::Desc,
            nulls,
        }
    }

    fn starts_with(&self, text: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(
            Func::starts_with(self.inner.clone(), coerce(text)?.inner).into(),
        ))
    }
    fn contains_text(&self, text: &Bound<'_, PyAny>) -> PyResult<Self> {
        let position =
            Func::cust(Alias::new("strpos")).args([self.inner.clone(), coerce(text)?.inner]);
        Ok(Self::from_rust(
            Expr::expr(position).gt(SimpleExpr::Constant(0i32.into())),
        ))
    }
    fn ends_with(&self, text: &Bound<'_, PyAny>) -> PyResult<Self> {
        let text = coerce(text)?.inner;
        let suffix = Func::cust(Alias::new("right"))
            .args([self.inner.clone(), Func::char_length(text.clone()).into()]);
        Ok(Self::from_rust(Expr::expr(suffix).eq(text)))
    }
    fn like(&self, pattern: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(
            self.inner.clone().like(options::pattern(pattern)?),
        ))
    }
    fn not_like(&self, pattern: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(
            self.inner.clone().not_like(options::pattern(pattern)?),
        ))
    }
    fn ilike(&self, pattern: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(
            Expr::expr(self.inner.clone()).ilike(options::pattern(pattern)?),
        ))
    }
    fn not_ilike(&self, pattern: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::from_rust(
            Expr::expr(self.inner.clone()).not_ilike(options::pattern(pattern)?),
        ))
    }

    /// Inspect this expression as the projection of a Rust-built SELECT.
    fn inspect(&self) -> Compiled {
        let (sql, values) = Query::select().expr(self.inner.clone()).build();
        Compiled { sql, values }
    }
    fn __repr__(&self) -> &'static str {
        "Expr(...)"
    }
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyExpr>()?;
    module.add_class::<PyCondition>()?;
    module.add_class::<LikePattern>()?;
    module.add_class::<AliasedExpr>()?;
    module.add_class::<OrderBy>()?;
    module.add_class::<Compiled>()?;
    module.add_function(wrap_pyfunction!(construct::col, module)?)?;
    module.add_function(wrap_pyfunction!(construct::bind, module)?)?;
    module.add_function(wrap_pyfunction!(construct::literal, module)?)?;
    module.add_function(wrap_pyfunction!(construct::call, module)?)?;
    module.add_function(wrap_pyfunction!(construct::tuple_expr, module)?)?;
    Ok(())
}
