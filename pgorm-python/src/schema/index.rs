use super::{
    statement::{PyDDL, Statement},
    table::table_name,
};
use crate::{
    UnsupportedCapabilityError, expressions::Compiled, identifiers::PyIdentifier,
    statements::PyTable,
};
use pgorm::pgorm_query::{
    Alias, Index, IndexCreateStatement, IndexOrder, IndexType, IntoIden, Values,
};
use pyo3::prelude::*;

#[derive(Clone, Debug)]
#[pyclass(name = "CreateIndex", module = "pgorm.schema", frozen, from_py_object)]
pub struct PyCreateIndex {
    pub(crate) inner: IndexCreateStatement,
}

#[pymethods]
impl PyCreateIndex {
    // [spec:pgorm:req:python.schema]
    #[new]
    #[pyo3(signature=(table, first, *, name=None))]
    fn new(
        table: &PyTable,
        first: &Bound<'_, PyAny>,
        name: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut inner = Index::create(table_name(table)?, PyIdentifier::new(first)?.alias());
        if let Some(name) = name {
            inner.name(PyIdentifier::new(name)?.alias());
        }
        Ok(Self { inner })
    }

    #[pyo3(signature=(name, *, descending=None))]
    fn column(&self, name: &Bound<'_, PyAny>, descending: Option<bool>) -> PyResult<Self> {
        let name = PyIdentifier::new(name)?.alias();
        let mut inner = self.inner.clone();
        match descending {
            None => {
                inner.col(name);
            }
            Some(false) => {
                inner.col((name, IndexOrder::Asc));
            }
            Some(true) => {
                inner.col((name, IndexOrder::Desc));
            }
        }
        Ok(Self { inner })
    }

    fn unique(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.unique();
        Self { inner }
    }

    fn nulls_not_distinct(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.unique().nulls_not_distinct();
        Self { inner }
    }

    fn if_not_exists(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.if_not_exists();
        Self { inner }
    }

    fn method(&self, name: &str) -> PyResult<Self> {
        let kind = match name {
            "btree" => IndexType::BTree,
            "hash" => IndexType::Hash,
            "gin" | "gist" | "spgist" | "brin" => IndexType::Custom(Alias::new(name).into_iden()),
            _ => {
                return Err(UnsupportedCapabilityError::new_err(
                    "unsupported index access method",
                ));
            }
        };
        let mut inner = self.inner.clone();
        inner.index_type(kind);
        Ok(Self { inner })
    }

    pub fn inspect(&self) -> Compiled {
        Compiled {
            sql: self.inner.to_string(),
            values: Values(Vec::new()),
        }
    }
}

#[pyfunction]
#[pyo3(signature=(table, name, *, if_exists=false))]
pub(super) fn drop_index(
    table: &PyTable,
    name: &Bound<'_, PyAny>,
    if_exists: bool,
) -> PyResult<PyDDL> {
    let mut inner = Index::drop(PyIdentifier::new(name)?.alias());
    inner.table(table_name(table)?);
    if if_exists {
        inner.if_exists();
    }
    Ok(PyDDL {
        inner: Statement::DropIndex(inner),
    })
}
