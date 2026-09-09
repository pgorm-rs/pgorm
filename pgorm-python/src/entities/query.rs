use pgorm::pgorm_query::IntoCondition;
use pyo3::{
    prelude::*,
    types::{PyInt, PyTuple},
};

use super::backend::{Change, Select, Terminal};
use crate::{
    errors::ConstructionError,
    expressions::{Compiled, OrderBy, PyCondition, PyExpr},
};

#[derive(Clone, Debug)]
#[pyclass(name = "EntityQuery", module = "pgorm", frozen, from_py_object)]
pub struct PyEntityQuery {
    pub(crate) inner: Select,
}

fn bound(value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<u64>> {
    let Some(value) = value.filter(|v| !v.is_none()) else {
        return Ok(None);
    };
    if !value.is_exact_instance_of::<PyInt>() {
        return Err(ConstructionError::new_err(
            "query bounds require an integer",
        ));
    }
    let number: i64 = value
        .extract()
        .map_err(|_| ConstructionError::new_err("query bound exceeds i64"))?;
    u64::try_from(number)
        .map(Some)
        .map_err(|_| ConstructionError::new_err("query bounds must be nonnegative"))
}

#[pymethods]
impl PyEntityQuery {
    #[getter]
    fn entity_name(&self) -> &str {
        &self.inner.info().name
    }

    fn filter(&self, predicate: &Bound<'_, PyAny>) -> PyResult<Self> {
        let condition = if let Ok(condition) = predicate.extract::<PyRef<'_, PyCondition>>() {
            condition.inner.clone()
        } else if let Ok(expr) = predicate.extract::<PyRef<'_, PyExpr>>() {
            expr.inner.clone().into_condition()
        } else {
            return Err(ConstructionError::new_err(
                "entity filters require an Expr or Condition",
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

    #[pyo3(signature = (value=None))]
    fn limit(&self, value: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.change(Change::Limit(bound(value)?)),
        })
    }
    #[pyo3(signature = (value=None))]
    fn offset(&self, value: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.change(Change::Offset(bound(value)?)),
        })
    }

    #[pyo3(signature = (*, terminal="all"))]
    fn inspect(&self, terminal: &str) -> PyResult<Compiled> {
        let terminal = match terminal {
            "all" => Terminal::All,
            "one" => Terminal::One,
            "one_opt" => Terminal::Optional,
            _ => return Err(ConstructionError::new_err("unknown entity terminal")),
        };
        let (sql, values) = self.inner.compile(terminal);
        if values.0.len() > 65535 {
            return Err(ConstructionError::new_err(
                "PostgreSQL supports at most 65535 query parameters",
            ));
        }
        Ok(Compiled { sql, values })
    }

    fn all<'py>(
        &self,
        py: Python<'py>,
        connection: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        super::io::select(py, connection, self.inner.clone(), Terminal::All)
    }
    fn one<'py>(
        &self,
        py: Python<'py>,
        connection: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        super::io::select(py, connection, self.inner.clone(), Terminal::One)
    }
    fn one_opt<'py>(
        &self,
        py: Python<'py>,
        connection: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        super::io::select(py, connection, self.inner.clone(), Terminal::Optional)
    }
    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "entity queries cannot be tested as Python booleans",
        ))
    }
    fn __repr__(&self) -> String {
        format!("EntityQuery({:?})", self.entity_name())
    }
}
