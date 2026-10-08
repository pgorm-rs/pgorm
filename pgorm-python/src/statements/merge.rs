//! MERGE over the Rust typestate: `merge()` returns a `PendingMerge`, which
//! has no WHEN arm and so nothing to inspect or execute, and its first arm
//! returns the `Merge` statement. Each arm's action is checked against the
//! kind of row the arm takes, as the Rust types check it.

use pgorm::pgorm_query::{MergeStatement, PendingMerge, Query};
use pyo3::{prelude::*, types::PyTuple};

use super::{
    common,
    merge_action::{matched, not_matched},
    table::PyTable,
    with::PyWith,
};
use crate::{errors::ConstructionError, expressions::Compiled};

type Arg<'a, 'py> = &'a Bound<'py, PyAny>;
type Opt<'a, 'py> = Option<&'a Bound<'py, PyAny>>;

// [spec:pgorm:req:python.statements+2]
/// `MERGE INTO target USING source ON condition` before its first WHEN arm.
#[pyclass(name = "PendingMerge", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyPendingMerge {
    inner: PendingMerge,
}

/// Begin a MERGE. A source that is a query is a common table expression the
/// statement names in `with_`, read here as a table.
#[pyfunction]
pub(crate) fn merge(
    target: PyRef<'_, PyTable>,
    source: PyRef<'_, PyTable>,
    on: &Bound<'_, PyAny>,
) -> PyResult<PyPendingMerge> {
    Ok(PyPendingMerge {
        inner: Query::merge(
            target.inner.clone(),
            source.inner.clone(),
            common::condition(on)?,
        ),
    })
}

#[pymethods]
impl PyPendingMerge {
    #[pyo3(signature = (action, *, condition=None))]
    fn when_matched(&self, action: Arg<'_, '_>, condition: Opt<'_, '_>) -> PyResult<PyMerge> {
        let (action, pending) = (matched(action)?, self.inner.clone());
        Ok(PyMerge::new(
            match condition.map(common::condition).transpose()? {
                Some(condition) => pending.when_matched_and(condition, action),
                None => pending.when_matched(action),
            },
        ))
    }

    #[pyo3(signature = (action, *, condition=None))]
    fn when_not_matched(&self, action: Arg<'_, '_>, condition: Opt<'_, '_>) -> PyResult<PyMerge> {
        let (action, pending) = (not_matched(action)?, self.inner.clone());
        Ok(PyMerge::new(
            match condition.map(common::condition).transpose()? {
                Some(condition) => pending.when_not_matched_and(condition, action),
                None => pending.when_not_matched(action),
            },
        ))
    }

    #[pyo3(signature = (action, *, condition=None))]
    fn when_not_matched_by_source(
        &self,
        action: Arg<'_, '_>,
        condition: Opt<'_, '_>,
    ) -> PyResult<PyMerge> {
        let (action, pending) = (matched(action)?, self.inner.clone());
        Ok(PyMerge::new(
            match condition.map(common::condition).transpose()? {
                Some(condition) => pending.when_not_matched_by_source_and(condition, action),
                None => pending.when_not_matched_by_source(action),
            },
        ))
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "SQL statements cannot be tested as Python booleans",
        ))
    }
    fn __repr__(&self) -> &'static str {
        "PendingMerge(...)"
    }
}

/// A MERGE with at least one WHEN arm. Within a kind of row, conditional arms
/// keep their call order and the one unconditional arm renders after them;
/// a later unconditional arm of the same kind replaces the earlier one.
#[pyclass(name = "Merge", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyMerge {
    pub inner: MergeStatement,
}

impl PyMerge {
    fn new(inner: MergeStatement) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyMerge {
    #[pyo3(signature = (action, *, condition=None))]
    fn when_matched(&self, action: Arg<'_, '_>, condition: Opt<'_, '_>) -> PyResult<Self> {
        let (action, mut next) = (matched(action)?, self.clone());
        match condition.map(common::condition).transpose()? {
            Some(condition) => next.inner.when_matched_and(condition, action),
            None => next.inner.when_matched(action),
        };
        Ok(next)
    }

    #[pyo3(signature = (action, *, condition=None))]
    fn when_not_matched(&self, action: Arg<'_, '_>, condition: Opt<'_, '_>) -> PyResult<Self> {
        let (action, mut next) = (not_matched(action)?, self.clone());
        match condition.map(common::condition).transpose()? {
            Some(condition) => next.inner.when_not_matched_and(condition, action),
            None => next.inner.when_not_matched(action),
        };
        Ok(next)
    }

    #[pyo3(signature = (action, *, condition=None))]
    fn when_not_matched_by_source(
        &self,
        action: Arg<'_, '_>,
        condition: Opt<'_, '_>,
    ) -> PyResult<Self> {
        let (action, mut next) = (matched(action)?, self.clone());
        match condition.map(common::condition).transpose()? {
            Some(condition) => next.inner.when_not_matched_by_source_and(condition, action),
            None => next.inner.when_not_matched_by_source(action),
        };
        Ok(next)
    }

    #[pyo3(signature = (*items, old_as=None, new_as=None))]
    fn returning(
        &self,
        items: &Bound<'_, PyTuple>,
        old_as: Option<&Bound<'_, PyAny>>,
        new_as: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut next = self.clone();
        next.inner
            .returning(common::returning(items, old_as, new_as)?);
        Ok(next)
    }

    /// `merge_action()` as the RETURNING list's first column. It has no
    /// expression form: PostgreSQL resolves it only in a MERGE's list.
    fn returning_action(&self) -> Self {
        let mut next = self.clone();
        next.inner.returning_action();
        next
    }

    fn with_(&self, clause: PyRef<'_, PyWith>) -> Self {
        let mut next = self.clone();
        next.inner.with(clause.inner.clone());
        next
    }

    fn only(&self) -> Self {
        let mut next = self.clone();
        next.inner.only();
        next
    }

    pub fn inspect(&self) -> PyResult<Compiled> {
        common::compiled(self.inner.build())
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "SQL statements cannot be tested as Python booleans",
        ))
    }
    fn __repr__(&self) -> &'static str {
        "Merge(...)"
    }
}
