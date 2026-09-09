use pgorm::pgorm_query::{DeleteStatement, Query, UpdateStatement};
use pyo3::{prelude::*, types::PyTuple};

use super::{common, table::PyTable};
use crate::{
    errors::ConstructionError,
    expressions::{Compiled, coerce},
    identifiers::PyIdentifier,
};

/// A runtime UPDATE with explicit predicates or all-rows intent.
#[pyclass(name = "Update", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyUpdate {
    pub inner: UpdateStatement,
    guarded: bool,
}

#[pymethods]
impl PyUpdate {
    #[new]
    fn new(table: PyRef<'_, PyTable>) -> Self {
        Self {
            inner: Query::update().table(table.inner.clone()).to_owned(),
            guarded: false,
        }
    }

    fn set(&self, column: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let name = PyIdentifier::new(column)?;
        if self
            .inner
            .get_values()
            .iter()
            .any(|(column, _)| column.to_string() == name.name)
        {
            return Err(ConstructionError::new_err("duplicate update assignment"));
        }
        let mut next = self.clone();
        next.inner.value(name.alias(), coerce(value)?.inner);
        Ok(next)
    }

    fn where_(&self, predicate: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut next = self.clone();
        next.inner.cond_where(common::condition(predicate)?);
        next.guarded = true;
        Ok(next)
    }

    fn all_rows(&self) -> Self {
        let mut next = self.clone();
        next.guarded = true;
        next
    }

    #[pyo3(signature = (*items))]
    fn returning(&self, items: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let mut next = self.clone();
        next.inner.returning(common::returning(items)?);
        Ok(next)
    }

    pub fn inspect(&self) -> PyResult<Compiled> {
        if !self.guarded {
            return Err(ConstructionError::new_err(
                "update requires where_ or explicit all_rows",
            ));
        }
        if self.inner.get_values().is_empty() {
            return Err(ConstructionError::new_err(
                "update requires at least one assignment",
            ));
        }
        common::compiled(self.inner.build())
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "SQL statements cannot be tested as Python booleans",
        ))
    }
    fn __repr__(&self) -> &'static str {
        "Update(...)"
    }
}

/// A runtime DELETE with explicit predicates or all-rows intent.
#[pyclass(name = "Delete", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyDelete {
    pub inner: DeleteStatement,
    guarded: bool,
}

#[pymethods]
impl PyDelete {
    #[new]
    fn new(table: PyRef<'_, PyTable>) -> Self {
        Self {
            inner: Query::delete().from_table(table.inner.clone()).to_owned(),
            guarded: false,
        }
    }

    fn where_(&self, predicate: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut next = self.clone();
        next.inner.cond_where(common::condition(predicate)?);
        next.guarded = true;
        Ok(next)
    }

    fn all_rows(&self) -> Self {
        let mut next = self.clone();
        next.guarded = true;
        next
    }

    #[pyo3(signature = (*items))]
    fn returning(&self, items: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let mut next = self.clone();
        next.inner.returning(common::returning(items)?);
        Ok(next)
    }

    pub fn inspect(&self) -> PyResult<Compiled> {
        if !self.guarded {
            return Err(ConstructionError::new_err(
                "delete requires where_ or explicit all_rows",
            ));
        }
        common::compiled(self.inner.build())
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "SQL statements cannot be tested as Python booleans",
        ))
    }
    fn __repr__(&self) -> &'static str {
        "Delete(...)"
    }
}
