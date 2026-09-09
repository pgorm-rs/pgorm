use std::collections::HashSet;

use pgorm::pgorm_query::{InsertStatement, Query};
use pyo3::{prelude::*, types::PyTuple};

use super::{common, conflict, table::PyTable};
use crate::{
    errors::ConstructionError,
    expressions::{Compiled, coerce},
    identifiers::PyIdentifier,
};

/// A runtime INSERT with explicit columns, row arity and default-row state.
#[pyclass(name = "Insert", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyInsert {
    pub inner: InsertStatement,
    columns: Vec<String>,
    rows: usize,
    defaults: bool,
}

#[pymethods]
impl PyInsert {
    #[new]
    fn new(table: PyRef<'_, PyTable>) -> Self {
        Self {
            inner: Query::insert().into_table(table.inner.clone()).to_owned(),
            columns: Vec::new(),
            rows: 0,
            defaults: false,
        }
    }

    #[pyo3(signature = (*columns))]
    fn columns(&self, columns: &Bound<'_, PyTuple>) -> PyResult<Self> {
        if self.rows > 0 || self.defaults {
            return Err(ConstructionError::new_err(
                "insert columns must be chosen before rows",
            ));
        }
        let mut names = HashSet::new();
        let columns = columns
            .iter()
            .map(|column| {
                let column = PyIdentifier::new(&column)?;
                if !names.insert(column.name.clone()) {
                    return Err(ConstructionError::new_err("duplicate insert column"));
                }
                Ok(column)
            })
            .collect::<PyResult<Vec<_>>>()?;
        if columns.is_empty() {
            return Err(ConstructionError::new_err(
                "use default_values for an insert without columns",
            ));
        }
        let mut next = self.clone();
        next.inner.columns(columns.iter().map(PyIdentifier::alias));
        next.columns = columns.into_iter().map(|column| column.name).collect();
        Ok(next)
    }

    #[pyo3(signature = (*values))]
    fn values(&self, values: &Bound<'_, PyTuple>) -> PyResult<Self> {
        if self.defaults || self.columns.is_empty() {
            return Err(ConstructionError::new_err(
                "choose insert columns before values",
            ));
        }
        let values = values
            .iter()
            .map(|value| coerce(&value).map(|expr| expr.inner))
            .collect::<PyResult<Vec<_>>>()?;
        let mut next = self.clone();
        next.inner
            .values(values)
            .map_err(|error| ConstructionError::new_err(error.to_string()))?;
        next.rows += 1;
        Ok(next)
    }

    fn default_values(&self) -> PyResult<Self> {
        if self.rows > 0 || !self.columns.is_empty() {
            return Err(ConstructionError::new_err(
                "default values cannot be mixed with insert columns or rows",
            ));
        }
        let mut next = self.clone();
        next.inner.or_default_values();
        next.defaults = true;
        Ok(next)
    }

    fn on_conflict(&self, action: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut next = self.clone();
        next.inner.on_conflict(conflict::clause(action)?);
        Ok(next)
    }

    #[pyo3(signature = (*items))]
    fn returning(&self, items: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let mut next = self.clone();
        next.inner.returning(common::returning(items)?);
        Ok(next)
    }

    pub fn inspect(&self) -> PyResult<Compiled> {
        if self.rows == 0 && !self.defaults {
            return Err(ConstructionError::new_err(
                "insert requires rows or explicit default_values",
            ));
        }
        common::compiled(self.inner.build())
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "SQL statements cannot be tested as Python booleans",
        ))
    }
    fn __repr__(&self) -> String {
        format!("Insert(rows={}, defaults={})", self.rows, self.defaults)
    }
}
