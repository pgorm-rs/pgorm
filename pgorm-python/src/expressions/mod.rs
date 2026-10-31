//! Composable, owned wrappers of the existing Rust expression builders.
//!
//! `Expr`'s methods are one `#[pymethods]` block per operator family, each
//! in its own module (PyO3's `multiple-pymethods`), as its stub is one mixin
//! per family.

mod arithmetic;
mod capabilities;
mod comparison;
mod compiled;
mod construct;
mod logic;
mod options;
mod patterns;
mod predicates;
mod ranges;
mod search;
mod shaping;
mod subscripts;
#[cfg(test)]
mod tests;

use pgorm::pgorm_query::{BinOper, SimpleExpr};
use pyo3::prelude::*;

pub(crate) use capabilities::operations as capabilities;
pub use compiled::Compiled;
pub use construct::PyCondition;
pub(crate) use construct::{coerce, require_expr};
pub use options::{Aliased, LikePattern, OrderBy};

// [spec:pgorm:req:bindings.expressions+2]
// [spec:pgorm:req:python.expressions+7]
// [spec:pgorm:req:bindings.statements]
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

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyExpr>()?;
    module.add_class::<PyCondition>()?;
    module.add_class::<LikePattern>()?;
    module.add_class::<Aliased>()?;
    module.add_class::<OrderBy>()?;
    module.add_class::<Compiled>()?;
    module.add_function(wrap_pyfunction!(construct::col, module)?)?;
    module.add_function(wrap_pyfunction!(construct::bind, module)?)?;
    module.add_function(wrap_pyfunction!(construct::literal, module)?)?;
    module.add_function(wrap_pyfunction!(construct::call, module)?)?;
    module.add_function(wrap_pyfunction!(construct::tuple_expr, module)?)?;
    module.add_function(wrap_pyfunction!(construct::domain_value, module)?)?;
    Ok(())
}
