use pyo3::{
    basic::CompareOp,
    prelude::*,
    types::{PyList, PyTuple},
};
use std::sync::Arc;

use super::{
    backend::{Comparison, EntityBackend, Members},
    metadata::EntityInfo,
};
use crate::{
    errors::{ConstructionError, LifecycleError},
    expressions::PyExpr,
    identifiers::PyIdentifier,
};

#[derive(Clone, Debug)]
#[pyclass(name = "EntityColumn", module = "pgorm", frozen, from_py_object)]
pub struct PyEntityColumn {
    pub(crate) entity: Arc<dyn EntityBackend>,
    #[pyo3(get)]
    pub(crate) name: String,
}

impl PyEntityColumn {
    fn comparison(&self, value: &Bound<'_, PyAny>, op: Comparison) -> PyResult<PyExpr> {
        let value = self.entity.info().column(&self.name)?.input.coerce(value)?;
        self.entity
            .compare(&self.name, op, value.inner)
            .map(PyExpr::from_rust)
            .map_err(|error| crate::errors::ConstructionError::new_err(error.to_string()))
    }

    /// Membership through the column's `ColumnTrait`: `is_in` / `is_not_in`
    /// over a list or tuple of values, each converted to the column's
    /// declared kind as a compared value is, or `in_subquery` /
    /// `not_in_subquery` over a `Select`'s rows, so a column whose domain is
    /// over an enum is a member as the enum, and so is what the subquery
    /// projects.
    // [spec:pgorm:req:python.entities+4]
    fn membership(&self, values: &Bound<'_, PyAny>, negated: bool) -> PyResult<PyExpr> {
        let members = if let Some(select) = crate::statements::subquery(values)? {
            Members::Subquery(Box::new(select))
        } else if values.is_exact_instance_of::<PyList>()
            || values.is_exact_instance_of::<PyTuple>()
        {
            let input = &self.entity.info().column(&self.name)?.input;
            let values = values
                .try_iter()?
                .map(|value| Ok(input.coerce(&value?)?.inner));
            Members::Values(values.collect::<PyResult<_>>()?)
        } else {
            return Err(ConstructionError::new_err(
                "membership requires a list or tuple of values, or a Select",
            ));
        };
        self.entity
            .membership(&self.name, negated, members)
            .map(PyExpr::from_rust)
            .map_err(|error| ConstructionError::new_err(error.to_string()))
    }
}

#[pymethods]
impl PyEntityColumn {
    #[getter]
    fn entity_name(&self) -> &str {
        &self.entity.info().name
    }

    fn describe<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("json")?.call_method1(
            "loads",
            (self
                .entity
                .info()
                .column(&self.name)?
                .describe()
                .to_string(),),
        )
    }

    fn expr(&self) -> PyResult<PyExpr> {
        self.entity
            .expression(&self.name)
            .map(PyExpr::from_rust)
            .map_err(|error| crate::errors::ConstructionError::new_err(error.to_string()))
    }

    fn eq(&self, value: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
        self.comparison(value, Comparison::Eq)
    }
    fn ne(&self, value: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
        self.comparison(value, Comparison::Ne)
    }
    fn gt(&self, value: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
        self.comparison(value, Comparison::Gt)
    }
    fn gte(&self, value: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
        self.comparison(value, Comparison::Ge)
    }
    fn lt(&self, value: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
        self.comparison(value, Comparison::Lt)
    }
    fn lte(&self, value: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
        self.comparison(value, Comparison::Le)
    }

    fn is_in(&self, values: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
        self.membership(values, false)
    }
    fn is_not_in(&self, values: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
        self.membership(values, true)
    }

    fn __richcmp__(&self, value: &Bound<'_, PyAny>, op: CompareOp) -> PyResult<PyExpr> {
        self.comparison(
            value,
            match op {
                CompareOp::Eq => Comparison::Eq,
                CompareOp::Ne => Comparison::Ne,
                CompareOp::Gt => Comparison::Gt,
                CompareOp::Ge => Comparison::Ge,
                CompareOp::Lt => Comparison::Lt,
                CompareOp::Le => Comparison::Le,
            },
        )
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "entity columns cannot be tested as Python booleans",
        ))
    }
    fn __repr__(&self) -> String {
        format!("EntityColumn({:?}, {:?})", self.entity_name(), self.name)
    }
}

pub(crate) fn name(input: &Bound<'_, PyAny>, owner: &Arc<EntityInfo>) -> PyResult<String> {
    let name = if let Ok(column) = input.extract::<PyRef<'_, PyEntityColumn>>() {
        if !Arc::ptr_eq(column.entity.info(), owner) {
            return Err(LifecycleError::new_err(
                "column belongs to a different entity registration",
            ));
        }
        column.name.clone()
    } else {
        PyIdentifier::new(input)?.name
    };
    owner.column(&name)?;
    Ok(name)
}
