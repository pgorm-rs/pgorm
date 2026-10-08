use pgorm::pgorm_query::{ColumnRef, Expr, FromItem};
use pyo3::prelude::*;

use super::table::PyTable;
use crate::{errors::ConstructionError, expressions::PyExpr, identifiers::PyIdentifier};

// [spec:pgorm:req:python.statements+3]
/// A FROM item that is not a named table, owned by Rust's `FromItem`. It is
/// always aliased, as PostgreSQL requires of every such item, so its columns
/// are qualified by a name the caller chose. `json_table` builds one.
#[pyclass(name = "FromItem", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyFromItem {
    pub(crate) inner: FromItem,
}

#[pymethods]
impl PyFromItem {
    fn col(&self, name: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
        Ok(PyExpr::from_rust(
            Expr::col(ColumnRef::TableColumn(
                self.inner.qualifier().clone(),
                PyIdentifier::new(name)?.name(),
            ))
            .into(),
        ))
    }

    fn star(&self) -> PyExpr {
        PyExpr::from_rust(
            Expr::col(ColumnRef::TableAsterisk(self.inner.qualifier().clone())).into(),
        )
    }

    #[getter]
    fn alias(&self) -> String {
        self.inner.qualifier().to_string()
    }

    fn __repr__(&self) -> String {
        format!("FromItem(alias={:?})", self.alias())
    }
}

/// The FROM item a `Table` or `FromItem` argument stands for.
pub(super) fn from_item(value: &Bound<'_, PyAny>) -> PyResult<FromItem> {
    if let Ok(table) = value.extract::<PyRef<'_, PyTable>>() {
        Ok(FromItem::Table(table.inner.clone()))
    } else if let Ok(item) = value.extract::<PyRef<'_, PyFromItem>>() {
        Ok(item.inner.clone())
    } else {
        Err(ConstructionError::new_err(
            "a FROM item requires a Table or FromItem",
        ))
    }
}
