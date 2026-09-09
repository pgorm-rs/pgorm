use pgorm::pgorm_query::{ConflictTarget, ConflictUpdate, OnConflict};
use pyo3::{prelude::*, types::PyTuple};

use super::common;
use crate::{errors::ConstructionError, expressions::coerce, identifiers::PyIdentifier};

/// A completed Rust ON CONFLICT clause.
#[pyclass(name = "Conflict", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyConflict {
    pub inner: OnConflict,
}

#[pymethods]
impl PyConflict {
    #[staticmethod]
    fn ignore() -> Self {
        Self {
            inner: OnConflict::do_nothing(),
        }
    }
}

/// A nonempty conflict target, before choosing its action.
#[pyclass(name = "ConflictTarget", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyConflictTarget {
    pub inner: ConflictTarget,
}

#[pymethods]
impl PyConflictTarget {
    #[new]
    #[pyo3(signature = (*columns))]
    fn new(columns: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let mut columns = columns.iter();
        let first = columns.next().ok_or_else(|| {
            ConstructionError::new_err("conflict target requires at least one column")
        })?;
        let mut inner = OnConflict::column(PyIdentifier::new(&first)?.alias());
        for column in columns {
            inner = inner.and_column(PyIdentifier::new(&column)?.alias());
        }
        Ok(Self { inner })
    }

    fn where_(&self, predicate: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.clone().cond_where(common::condition(predicate)?),
        })
    }

    fn ignore(&self) -> PyConflict {
        PyConflict {
            inner: self.inner.clone().do_nothing(),
        }
    }

    #[pyo3(signature = (*columns))]
    fn update(&self, columns: &Bound<'_, PyTuple>) -> PyResult<PyConflictUpdate> {
        let mut columns = columns.iter();
        let first = columns.next().ok_or_else(|| {
            ConstructionError::new_err("conflict update requires at least one column")
        })?;
        let mut inner = self
            .inner
            .clone()
            .update_column(PyIdentifier::new(&first)?.alias());
        for column in columns {
            inner = inner.update_column(PyIdentifier::new(&column)?.alias());
        }
        Ok(PyConflictUpdate { inner })
    }

    fn set(
        &self,
        column: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<PyConflictUpdate> {
        Ok(PyConflictUpdate {
            inner: self
                .inner
                .clone()
                .value(PyIdentifier::new(column)?.alias(), coerce(value)?.inner),
        })
    }
}

/// A nonempty Rust DO UPDATE assignment set, with optional update predicate.
#[pyclass(name = "ConflictUpdate", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyConflictUpdate {
    pub inner: ConflictUpdate,
}

#[pymethods]
impl PyConflictUpdate {
    fn set(&self, column: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: self
                .inner
                .clone()
                .value(PyIdentifier::new(column)?.alias(), coerce(value)?.inner),
        })
    }

    fn update(&self, column: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: self
                .inner
                .clone()
                .update_column(PyIdentifier::new(column)?.alias()),
        })
    }

    fn where_(&self, predicate: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.clone().cond_where(common::condition(predicate)?),
        })
    }
}

pub(super) fn clause(value: &Bound<'_, PyAny>) -> PyResult<OnConflict> {
    if let Ok(value) = value.extract::<PyRef<'_, PyConflict>>() {
        Ok(value.inner.clone())
    } else if let Ok(value) = value.extract::<PyRef<'_, PyConflictUpdate>>() {
        Ok(value.inner.clone().into())
    } else {
        Err(ConstructionError::new_err(
            "on_conflict requires a completed conflict action",
        ))
    }
}
