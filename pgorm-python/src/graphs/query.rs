use super::{
    backend::{Change, CursorPlan, Query},
    cursor::PyGraphCursor,
};
use crate::{
    errors::ConstructionError,
    expressions::{Compiled, OrderBy, PyCondition, PyExpr},
};
use pgorm::pgorm_query::{Alias, Expr, IntoCondition};
use pyo3::{
    prelude::*,
    types::{PyInt, PyTuple},
};

#[derive(Clone, Debug)]
#[pyclass(name = "GraphQuery", module = "pgorm", frozen, from_py_object)]
pub struct PyGraphQuery {
    pub(crate) inner: Query,
}

#[pymethods]
impl PyGraphQuery {
    #[getter]
    fn graph_name(&self) -> &str {
        &self.inner.info().name
    }

    fn col(&self, source: &Bound<'_, PyAny>, column: &str) -> PyResult<PyExpr> {
        if !source.is_exact_instance_of::<PyInt>() {
            return Err(ConstructionError::new_err(
                "graph source index requires an integer",
            ));
        }
        let source: usize = source
            .extract()
            .map_err(|_| ConstructionError::new_err("invalid graph source index"))?;
        let descriptor = self
            .inner
            .info()
            .bindings
            .sources
            .get(source)
            .ok_or_else(|| {
                ConstructionError::new_err("source is outside the registered graph shape")
            })?;
        descriptor.entity.column(column)?;
        let qualifier = if source == 0 {
            &descriptor.entity.table
        } else {
            &self.inner.aliases()[source - 1]
        };
        Ok(PyExpr::from_rust(
            Expr::col((Alias::new(qualifier), Alias::new(column))).into(),
        ))
    }

    fn filter(&self, predicate: &Bound<'_, PyAny>) -> PyResult<Self> {
        let condition = if let Ok(condition) = predicate.extract::<PyRef<'_, PyCondition>>() {
            condition.inner.clone()
        } else if let Ok(expr) = predicate.extract::<PyRef<'_, PyExpr>>() {
            expr.inner.clone().into_condition()
        } else {
            return Err(ConstructionError::new_err(
                "graph filters require an Expr or Condition",
            ));
        };
        Ok(Self {
            inner: self.inner.change(Change::Filter(condition)),
        })
    }

    #[pyo3(signature = (*ordering))]
    fn order_by(&self, ordering: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let mut inner = self.inner.clone();
        for order in ordering {
            let order = order.extract::<PyRef<'_, OrderBy>>().map_err(|_| {
                ConstructionError::new_err("order_by requires typed OrderBy values")
            })?;
            inner = inner.change(Change::Order(
                order.expr.inner.clone(),
                order.direction.rust_order(),
                order.nulls.map(|n| n.rust_nulls()),
            ));
        }
        Ok(Self { inner })
    }

    #[pyo3(signature = (*, terminal="all"))]
    fn inspect(&self, terminal: &str) -> PyResult<Compiled> {
        let optional = match terminal {
            "all" => false,
            "one_opt" => true,
            _ => return Err(ConstructionError::new_err("unknown graph terminal")),
        };
        let (sql, values) = self.inner.compile(optional);
        if values.0.len() > 65535 {
            return Err(ConstructionError::new_err(
                "PostgreSQL supports at most 65535 query parameters",
            ));
        }
        Ok(Compiled { sql, values })
    }

    fn cursor(&self, column: &str) -> PyResult<PyGraphCursor> {
        self.inner.info().bindings.sources[0]
            .entity
            .column(column)?;
        Ok(PyGraphCursor {
            query: self.inner.clone(),
            plan: CursorPlan {
                column: column.to_owned(),
                before: None,
                after: None,
                descending: false,
                window: None,
            },
        })
    }

    fn all<'py>(
        &self,
        py: Python<'py>,
        connection: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        super::io::read(py, connection, self.inner.clone(), super::io::Read::All)
    }

    fn one_opt<'py>(
        &self,
        py: Python<'py>,
        connection: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        super::io::read(
            py,
            connection,
            self.inner.clone(),
            super::io::Read::Optional,
        )
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "graph queries cannot be tested as Python booleans",
        ))
    }
}
