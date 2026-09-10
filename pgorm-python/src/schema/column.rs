use super::types::PyDataType;
use crate::{expressions, identifiers::PyIdentifier};
use pgorm::pgorm_query::ColumnDef;
use pyo3::prelude::*;

#[derive(Clone, Debug)]
#[pyclass(name = "ColumnDef", module = "pgorm.schema", frozen, from_py_object)]
pub struct PyColumnDef {
    pub(crate) inner: ColumnDef,
}

#[pymethods]
impl PyColumnDef {
    // [spec:pgorm:req:python.schema]
    #[new]
    fn new(name: &Bound<'_, PyAny>, kind: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: ColumnDef::new_with_type(
                PyIdentifier::new(name)?.alias(),
                PyDataType::coerce(kind)?.inner,
            ),
        })
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.get_column_name()
    }

    fn not_null(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.not_null();
        Self { inner }
    }
    fn null(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.null();
        Self { inner }
    }
    fn primary_key(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.primary_key();
        Self { inner }
    }
    fn unique(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.unique_key();
        Self { inner }
    }
    fn auto_increment(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.auto_increment();
        Self { inner }
    }
    fn default(&self, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut inner = self.inner.clone();
        inner.default(expressions::coerce(value)?.inner);
        Ok(Self { inner })
    }
    fn check(&self, condition: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut inner = self.inner.clone();
        inner.check(expressions::require_expr(condition)?.inner);
        Ok(Self { inner })
    }
    fn generated(&self, expression: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut inner = self.inner.clone();
        inner.generated(expressions::require_expr(expression)?.inner, true);
        Ok(Self { inner })
    }
}
