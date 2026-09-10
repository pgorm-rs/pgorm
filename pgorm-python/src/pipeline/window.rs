use pgorm::pipeline as pl;
use pyo3::{prelude::*, types::PyTuple};

use super::{construct::integer, expression::PyPipelineExpr};
use crate::errors::ConstructionError;

/// A real Rust Over specification; binder expressions cannot enter it.
#[pyclass(
    name = "PipelineOver",
    module = "pgorm.pipeline",
    frozen,
    from_py_object
)]
#[derive(Clone, Debug, Default)]
pub struct PyOver {
    pub(super) inner: pl::Over,
}

pub(super) fn expressions(values: &Bound<'_, PyTuple>) -> PyResult<Vec<pl::Expr<'static>>> {
    values
        .iter()
        .map(|value| PyPipelineExpr::coerce(&value)?.unbound())
        .collect()
}

#[pymethods]
impl PyOver {
    #[new]
    fn new() -> Self {
        Self { inner: pl::over() }
    }

    #[pyo3(signature = (*keys))]
    fn by(&self, keys: &Bound<'_, PyTuple>) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.clone().by(expressions(keys)?),
        })
    }

    #[pyo3(signature = (*keys))]
    fn sort_by(&self, keys: &Bound<'_, PyTuple>) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.clone().sort_by(expressions(keys)?),
        })
    }

    #[pyo3(signature = (start=None, end=None))]
    fn rows(
        &self,
        start: Option<&Bound<'_, PyAny>>,
        end: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.clone().rows(
                start.map(integer).transpose()?,
                end.map(integer).transpose()?,
            ),
        })
    }

    #[pyo3(signature = (start=None, end=None))]
    fn range(
        &self,
        start: Option<&Bound<'_, PyAny>>,
        end: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.clone().range(
                start.map(integer).transpose()?,
                end.map(integer).transpose()?,
            ),
        })
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "pipeline window specifications cannot be tested as booleans",
        ))
    }
}
