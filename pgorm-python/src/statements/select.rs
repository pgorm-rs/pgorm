use pgorm::pgorm_query::{Asterisk, JoinType, Query, SelectStatement};
use pyo3::{prelude::*, types::PyTuple};

use super::{common, table::PyTable};
use crate::{
    errors::ConstructionError,
    expressions::{Compiled, OrderBy},
};

/// Join kinds which require an ON condition; CROSS JOIN has its own method.
#[pyclass(name = "Join", module = "pgorm", eq, from_py_object)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    Inner,
    Left,
    Right,
    Full,
}

impl Join {
    fn rust_join(self) -> JoinType {
        match self {
            Self::Inner => JoinType::InnerJoin,
            Self::Left => JoinType::LeftJoin,
            Self::Right => JoinType::RightJoin,
            Self::Full => JoinType::FullOuterJoin,
        }
    }
}

// [spec:pgorm:req:python.statements]
/// An immutable runtime SELECT backed by a real Rust SelectStatement.
#[pyclass(name = "Select", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PySelect {
    pub inner: SelectStatement,
}

#[pymethods]
impl PySelect {
    #[new]
    #[pyo3(signature = (*items))]
    pub fn new(items: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let mut inner = Query::select();
        if items.is_empty() {
            inner.column(Asterisk);
        } else {
            common::project(&mut inner, items)?;
        }
        Ok(Self { inner })
    }

    #[pyo3(signature = (*items))]
    fn select(&self, items: &Bound<'_, PyTuple>) -> PyResult<Self> {
        if items.is_empty() {
            return Err(ConstructionError::new_err(
                "select requires at least one projection",
            ));
        }
        let mut next = self.clone();
        next.inner.clear_selects();
        common::project(&mut next.inner, items)?;
        Ok(next)
    }

    fn from_(&self, table: PyRef<'_, PyTable>) -> Self {
        let mut next = self.clone();
        next.inner.from(table.inner.clone());
        next
    }

    fn where_(&self, predicate: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut next = self.clone();
        next.inner.cond_where(common::condition(predicate)?);
        Ok(next)
    }

    #[pyo3(signature = (table, on, *, kind=Join::Inner))]
    fn join(&self, table: PyRef<'_, PyTable>, on: &Bound<'_, PyAny>, kind: Join) -> PyResult<Self> {
        let mut next = self.clone();
        next.inner.join(
            kind.rust_join(),
            table.inner.clone(),
            common::condition(on)?,
        );
        Ok(next)
    }

    fn cross_join(&self, table: PyRef<'_, PyTable>) -> Self {
        let mut next = self.clone();
        next.inner.cross_join(table.inner.clone());
        next
    }

    #[pyo3(signature = (*expressions))]
    fn group_by(&self, expressions: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let mut next = self.clone();
        next.inner.add_group_by(common::expressions(expressions)?);
        Ok(next)
    }

    fn having(&self, predicate: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut next = self.clone();
        next.inner.cond_having(common::condition(predicate)?);
        Ok(next)
    }

    #[pyo3(signature = (*orderings))]
    fn order_by(&self, orderings: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let mut next = self.clone();
        for ordering in orderings.iter() {
            let ordering = ordering
                .extract::<PyRef<'_, OrderBy>>()
                .map_err(|_| ConstructionError::new_err("order_by requires OrderBy expressions"))?;
            match ordering.nulls {
                Some(nulls) => next.inner.order_by_expr_with_nulls(
                    ordering.expr.inner.clone(),
                    ordering.direction.rust_order(),
                    nulls.rust_nulls(),
                ),
                None => next
                    .inner
                    .order_by_expr(ordering.expr.inner.clone(), ordering.direction.rust_order()),
            };
        }
        Ok(next)
    }

    fn limit(&self, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut next = self.clone();
        if value.is_none() {
            next.inner.reset_limit();
        } else {
            next.inner.limit(common::unsigned(value)?);
        }
        Ok(next)
    }

    fn offset(&self, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut next = self.clone();
        if value.is_none() {
            next.inner.reset_offset();
        } else {
            next.inner.offset(common::unsigned(value)?);
        }
        Ok(next)
    }

    fn distinct(&self) -> Self {
        let mut next = self.clone();
        next.inner.distinct();
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
        "Select(...)"
    }
}
