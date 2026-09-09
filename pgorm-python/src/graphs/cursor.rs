use super::backend::{Boundary, CursorPlan, Query};
use crate::{
    entities::{metadata::ColumnInfo, query::bound},
    errors::ConstructionError,
};
use pyo3::{prelude::*, types::PyTuple};

#[derive(Clone, Debug)]
#[pyclass(name = "GraphCursor", module = "pgorm", frozen, from_py_object)]
pub struct PyGraphCursor {
    pub(crate) query: Query,
    pub(crate) plan: CursorPlan,
}

impl PyGraphCursor {
    fn columns(&self) -> PyResult<Vec<&ColumnInfo>> {
        let sources = &self.query.info().bindings.sources;
        let root = &sources[0].entity;
        let mut columns = vec![root.column(&self.plan.column)?];
        for (source_index, source) in sources.iter().enumerate() {
            for key in &source.entity.primary_keys {
                if source_index != 0 || key != &self.plan.column {
                    columns.push(source.entity.column(key)?);
                }
            }
        }
        Ok(columns)
    }

    fn primary(&self, before: bool, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let column = self.query.info().bindings.sources[0]
            .entity
            .column(&self.plan.column)?;
        let bound = Boundary {
            values: vec![column.input.coerce(value)?.inner],
            full: false,
        };
        let mut cursor = self.clone();
        if before {
            cursor.plan.before = Some(bound);
        } else {
            cursor.plan.after = Some(bound);
        }
        Ok(cursor)
    }

    fn full(&self, before: bool, values: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let columns = self.columns()?;
        if columns.len() != values.len() {
            return Err(ConstructionError::new_err(format!(
                "full cursor boundary requires {} values",
                columns.len()
            )));
        }
        let values = columns
            .into_iter()
            .zip(values)
            .map(|(column, value)| Ok(column.input.coerce(&value)?.inner))
            .collect::<PyResult<_>>()?;
        let bound = Boundary { values, full: true };
        let mut cursor = self.clone();
        if before {
            cursor.plan.before = Some(bound);
        } else {
            cursor.plan.after = Some(bound);
        }
        Ok(cursor)
    }

    fn window(&self, last: bool, count: &Bound<'_, PyAny>) -> PyResult<Self> {
        let count = bound(Some(count))?
            .ok_or_else(|| ConstructionError::new_err("cursor window requires a row count"))?;
        let mut cursor = self.clone();
        cursor.plan.window = Some((last, count));
        Ok(cursor)
    }
}

#[pymethods]
impl PyGraphCursor {
    fn before(&self, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.primary(true, value)
    }
    fn after(&self, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.primary(false, value)
    }
    #[pyo3(signature = (*values))]
    fn before_with(&self, values: &Bound<'_, PyTuple>) -> PyResult<Self> {
        self.full(true, values)
    }
    #[pyo3(signature = (*values))]
    fn after_with(&self, values: &Bound<'_, PyTuple>) -> PyResult<Self> {
        self.full(false, values)
    }
    fn first(&self, count: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.window(false, count)
    }
    fn last(&self, count: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.window(true, count)
    }

    fn asc(&self) -> Self {
        let mut cursor = self.clone();
        cursor.plan.descending = false;
        cursor
    }
    fn desc(&self) -> Self {
        let mut cursor = self.clone();
        cursor.plan.descending = true;
        cursor
    }
    fn all<'py>(
        &self,
        py: Python<'py>,
        connection: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        super::io::read(
            py,
            connection,
            self.query.clone(),
            super::io::Read::Cursor(self.plan.clone()),
        )
    }
    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "graph cursors cannot be tested as Python booleans",
        ))
    }
}
