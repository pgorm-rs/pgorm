//! The actions a MERGE arm takes: on a target row (`MergeUpdate`,
//! `MatchedAction`) or on a source row no target row matched (`MergeInsert`,
//! `NotMatchedAction`). Both assignment lists are non-empty by construction,
//! each column arriving with its value.

use pgorm::pgorm_query::{MatchedAction, MergeInsert, MergeUpdate, NotMatchedAction, Overriding};
use pyo3::{exceptions::PyTypeError, prelude::*};

use crate::{expressions::coerce, identifiers::PyIdentifier};

/// What a matched or not-matched-by-source arm does besides an update.
#[pyclass(name = "MatchedAction", module = "pgorm", eq, from_py_object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PyMatchedAction {
    Delete,
    DoNothing,
}

/// What a not-matched arm does besides an insert of values.
#[pyclass(name = "NotMatchedAction", module = "pgorm", eq, from_py_object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PyNotMatchedAction {
    InsertDefaultValues,
    DoNothing,
}

/// What an inserted row's identity columns do with a supplied value.
#[pyclass(name = "Overriding", module = "pgorm", eq, from_py_object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PyOverriding {
    SystemValue,
    UserValue,
}

/// `UPDATE SET column = value, ..` for a target row.
#[pyclass(name = "MergeUpdate", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyMergeUpdate {
    inner: MergeUpdate,
}

#[pymethods]
impl PyMergeUpdate {
    #[new]
    fn new(column: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let column = PyIdentifier::new(column)?.name();
        Ok(Self {
            inner: MergeUpdate::value(column, coerce(value)?.inner),
        })
    }

    fn and_value(&self, column: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let column = PyIdentifier::new(column)?.name();
        let value = coerce(value)?.inner;
        Ok(Self {
            inner: self.inner.clone().and_value(column, value),
        })
    }

    fn __repr__(&self) -> &'static str {
        "MergeUpdate(...)"
    }
}

/// `INSERT (column, ..) VALUES (value, ..)` for a source row.
#[pyclass(name = "MergeInsert", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyMergeInsert {
    inner: MergeInsert,
}

#[pymethods]
impl PyMergeInsert {
    #[new]
    fn new(column: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let column = PyIdentifier::new(column)?.name();
        Ok(Self {
            inner: MergeInsert::value(column, coerce(value)?.inner),
        })
    }

    fn and_value(&self, column: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let column = PyIdentifier::new(column)?.name();
        let value = coerce(value)?.inner;
        Ok(Self {
            inner: self.inner.clone().and_value(column, value),
        })
    }

    /// `OVERRIDING SYSTEM VALUE` or `OVERRIDING USER VALUE`; the last call wins.
    fn overriding(&self, overriding: PyOverriding) -> Self {
        let overriding = match overriding {
            PyOverriding::SystemValue => Overriding::SystemValue,
            PyOverriding::UserValue => Overriding::UserValue,
        };
        Self {
            inner: self.inner.clone().overriding(overriding),
        }
    }

    fn __repr__(&self) -> &'static str {
        "MergeInsert(...)"
    }
}

/// The action of an arm that takes a target row. An insert has no target row
/// to take, so it is refused here as the Rust types refuse it.
pub(super) fn matched(action: &Bound<'_, PyAny>) -> PyResult<MatchedAction> {
    if let Ok(update) = action.extract::<PyRef<'_, PyMergeUpdate>>() {
        return Ok(update.inner.clone().into());
    }
    match action.extract::<PyMatchedAction>() {
        Ok(PyMatchedAction::Delete) => Ok(MatchedAction::Delete),
        Ok(PyMatchedAction::DoNothing) => Ok(MatchedAction::DoNothing),
        Err(_) => Err(PyTypeError::new_err(
            "an arm on a target row takes MergeUpdate, MatchedAction.Delete or \
             MatchedAction.DoNothing",
        )),
    }
}

/// The action of an arm that takes a source row no target row matched: an
/// insert or nothing, never an update or a delete.
pub(super) fn not_matched(action: &Bound<'_, PyAny>) -> PyResult<NotMatchedAction> {
    if let Ok(insert) = action.extract::<PyRef<'_, PyMergeInsert>>() {
        return Ok(insert.inner.clone().into());
    }
    match action.extract::<PyNotMatchedAction>() {
        Ok(PyNotMatchedAction::InsertDefaultValues) => Ok(NotMatchedAction::InsertDefaultValues),
        Ok(PyNotMatchedAction::DoNothing) => Ok(NotMatchedAction::DoNothing),
        Err(_) => Err(PyTypeError::new_err(
            "a not-matched arm takes MergeInsert, NotMatchedAction.InsertDefaultValues or \
             NotMatchedAction.DoNothing",
        )),
    }
}
