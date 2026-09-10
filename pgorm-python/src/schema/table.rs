use super::{
    column::PyColumnDef,
    statement::{PyDDL, Statement},
};
use crate::{
    errors::ConstructionError, expressions, expressions::Compiled, identifiers::PyIdentifier,
    statements::PyTable,
};
use pgorm::pgorm_query::{Index, Table, TableCreateStatement, TableName, Values};
use pyo3::{prelude::*, types::PyTuple};

pub(super) fn table_name(table: &PyTable) -> PyResult<TableName> {
    if table.inner.alias.is_some() {
        return Err(ConstructionError::new_err(
            "DDL table targets cannot have an alias",
        ));
    }
    Ok(table.inner.name.clone())
}

#[derive(Clone, Debug)]
#[pyclass(name = "CreateTable", module = "pgorm.schema", frozen, from_py_object)]
pub struct PyCreateTable {
    pub(crate) inner: TableCreateStatement,
}

#[pymethods]
impl PyCreateTable {
    // [spec:pgorm:req:python.schema]
    #[new]
    fn new(table: &PyTable) -> PyResult<Self> {
        Ok(Self {
            inner: Table::create(table_name(table)?),
        })
    }

    fn column(&self, column: &PyColumnDef) -> Self {
        let mut inner = self.inner.clone();
        inner.col(column.inner.clone());
        Self { inner }
    }

    fn if_not_exists(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.if_not_exists();
        Self { inner }
    }

    #[pyo3(signature=(first, *rest))]
    fn primary_key(&self, first: &Bound<'_, PyAny>, rest: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let mut key = Index::create(
            self.inner.get_table_name().clone(),
            PyIdentifier::new(first)?.alias(),
        );
        for column in rest {
            key.col(PyIdentifier::new(&column)?.alias());
        }
        let mut inner = self.inner.clone();
        inner.primary_key(&mut key);
        Ok(Self { inner })
    }

    #[pyo3(signature=(first, *rest, name=None, nulls_not_distinct=false))]
    fn unique(
        &self,
        first: &Bound<'_, PyAny>,
        rest: &Bound<'_, PyTuple>,
        name: Option<&Bound<'_, PyAny>>,
        nulls_not_distinct: bool,
    ) -> PyResult<Self> {
        let mut key = Index::create(
            self.inner.get_table_name().clone(),
            PyIdentifier::new(first)?.alias(),
        );
        for column in rest {
            key.col(PyIdentifier::new(&column)?.alias());
        }
        key.unique();
        if let Some(name) = name {
            key.name(PyIdentifier::new(name)?.alias());
        }
        if nulls_not_distinct {
            key.nulls_not_distinct();
        }
        let mut inner = self.inner.clone();
        inner.index(&mut key);
        Ok(Self { inner })
    }

    fn check(&self, condition: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut inner = self.inner.clone();
        inner.check(expressions::require_expr(condition)?.inner);
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
#[pyo3(signature=(table, *, if_exists=false, cascade=false))]
pub(super) fn drop_table(table: &PyTable, if_exists: bool, cascade: bool) -> PyResult<PyDDL> {
    let mut inner = Table::drop(table_name(table)?);
    if if_exists {
        inner.if_exists();
    }
    if cascade {
        inner.cascade();
    }
    Ok(PyDDL {
        inner: Statement::DropTable(inner),
    })
}

#[pyfunction]
pub(super) fn rename_table(table: &PyTable, name: &Bound<'_, PyAny>) -> PyResult<PyDDL> {
    Ok(PyDDL {
        inner: Statement::RenameTable(Table::rename(
            table_name(table)?,
            PyIdentifier::new(name)?.alias(),
        )),
    })
}

#[pyfunction]
pub(super) fn rename_column(
    table: &PyTable,
    name: &Bound<'_, PyAny>,
    new_name: &Bound<'_, PyAny>,
) -> PyResult<PyDDL> {
    Ok(PyDDL {
        inner: Statement::RenameColumn(Table::rename_column(
            table_name(table)?,
            PyIdentifier::new(name)?.alias(),
            PyIdentifier::new(new_name)?.alias(),
        )),
    })
}

#[pyfunction]
pub(super) fn truncate(table: &PyTable) -> PyResult<PyDDL> {
    Ok(PyDDL {
        inner: Statement::Truncate(Table::truncate(table_name(table)?)),
    })
}

#[pyfunction]
#[pyo3(signature=(table, column, *, if_not_exists=false))]
pub(super) fn add_column(
    table: &PyTable,
    column: &PyColumnDef,
    if_not_exists: bool,
) -> PyResult<PyDDL> {
    let pending = Table::alter(table_name(table)?);
    let ready = if if_not_exists {
        pending.add_column_if_not_exists(column.inner.clone())
    } else {
        pending.add_column(column.inner.clone())
    };
    Ok(PyDDL {
        inner: Statement::AlterTable(ready),
    })
}

#[pyfunction]
pub(super) fn modify_column(table: &PyTable, column: &PyColumnDef) -> PyResult<PyDDL> {
    Ok(PyDDL {
        inner: Statement::AlterTable(
            Table::alter(table_name(table)?).modify_column(column.inner.clone()),
        ),
    })
}

#[pyfunction]
pub(super) fn drop_column(table: &PyTable, name: &Bound<'_, PyAny>) -> PyResult<PyDDL> {
    Ok(PyDDL {
        inner: Statement::AlterTable(
            Table::alter(table_name(table)?).drop_column(PyIdentifier::new(name)?.alias()),
        ),
    })
}
