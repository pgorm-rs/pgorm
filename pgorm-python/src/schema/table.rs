use super::{
    column::PyColumnDef,
    statement::{PyDDL, Statement},
};
use crate::{
    errors::ConstructionError, expressions, expressions::Compiled, identifiers::PyIdentifier,
    statements::PyTable,
};
use pgorm::pgorm_query::{
    ConstraintChange, NotNullConstraint, Table, TableCreateStatement, TableKey, TableName, Values,
};
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
    // A table has one primary key: a later call replaces the key an earlier
    // one declared, as the native builder's one slot does.
    // [spec:pgorm:req:python.schema]
    fn primary_key(&self, first: &Bound<'_, PyAny>, rest: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let mut key = TableKey::new(PyIdentifier::new(first)?.name());
        for column in rest {
            key = key.col(PyIdentifier::new(&column)?.name());
        }
        let mut inner = self.inner.clone();
        inner.primary_key(key);
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
        let mut key = TableKey::new(PyIdentifier::new(first)?.name());
        if nulls_not_distinct {
            key = key.nulls_not_distinct();
        }
        for column in rest {
            key = key.col(PyIdentifier::new(&column)?.name());
        }
        if let Some(name) = name {
            key = key.name(PyIdentifier::new(name)?.name());
        }
        let mut inner = self.inner.clone();
        inner.unique(key);
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
            PyIdentifier::new(name)?.name(),
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
            PyIdentifier::new(name)?.name(),
            PyIdentifier::new(new_name)?.name(),
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

/// `ALTER TABLE ... ADD PRIMARY KEY (...)`: the key `CreateTable.primary_key`
/// declares, added to a table that exists.
// [spec:pgorm:req:python.schema]
#[pyfunction]
#[pyo3(signature=(table, first, *rest))]
pub(super) fn add_primary_key(
    table: &PyTable,
    first: &Bound<'_, PyAny>,
    rest: &Bound<'_, PyTuple>,
) -> PyResult<PyDDL> {
    let mut key = TableKey::new(PyIdentifier::new(first)?.name());
    for column in rest {
        key = key.col(PyIdentifier::new(&column)?.name());
    }
    Ok(PyDDL {
        inner: Statement::AlterTable(Table::alter(table_name(table)?).add_primary_key(key)),
    })
}

/// `ALTER TABLE ... ADD UNIQUE (...)`: the key `CreateTable.unique` declares,
/// added to a table that exists.
// [spec:pgorm:req:python.schema]
#[pyfunction]
#[pyo3(signature=(table, first, *rest, name=None, nulls_not_distinct=false))]
pub(super) fn add_unique(
    table: &PyTable,
    first: &Bound<'_, PyAny>,
    rest: &Bound<'_, PyTuple>,
    name: Option<&Bound<'_, PyAny>>,
    nulls_not_distinct: bool,
) -> PyResult<PyDDL> {
    let mut key = TableKey::new(PyIdentifier::new(first)?.name());
    if nulls_not_distinct {
        key = key.nulls_not_distinct();
    }
    for column in rest {
        key = key.col(PyIdentifier::new(&column)?.name());
    }
    if let Some(name) = name {
        key = key.name(PyIdentifier::new(name)?.name());
    }
    Ok(PyDDL {
        inner: Statement::AlterTable(Table::alter(table_name(table)?).add_unique(key)),
    })
}

#[pyfunction]
pub(super) fn drop_column(table: &PyTable, name: &Bound<'_, PyAny>) -> PyResult<PyDDL> {
    Ok(PyDDL {
        inner: Statement::AlterTable(
            Table::alter(table_name(table)?).drop_column(PyIdentifier::new(name)?.name()),
        ),
    })
}

/// `ALTER TABLE ... ALTER COLUMN ... SET EXPRESSION AS (...)`: a generated
/// column's new expression, which the rows already written take.
// [spec:pgorm:req:python.schema]
#[pyfunction]
pub(super) fn set_expression(
    table: &PyTable,
    name: &Bound<'_, PyAny>,
    expression: &Bound<'_, PyAny>,
) -> PyResult<PyDDL> {
    Ok(PyDDL {
        inner: Statement::AlterTable(Table::alter(table_name(table)?).set_expression(
            PyIdentifier::new(name)?.name(),
            expressions::require_expr(expression)?.inner,
        )),
    })
}

/// `ALTER TABLE ... ALTER COLUMN ... DROP EXPRESSION [IF EXISTS]`: a stored
/// generated column made plain, keeping its values.
// [spec:pgorm:req:python.schema]
#[pyfunction]
#[pyo3(signature=(table, name, *, if_exists=false))]
pub(super) fn drop_expression(
    table: &PyTable,
    name: &Bound<'_, PyAny>,
    if_exists: bool,
) -> PyResult<PyDDL> {
    let pending = Table::alter(table_name(table)?);
    let column = PyIdentifier::new(name)?.name();
    let ready = if if_exists {
        pending.drop_expression_if_exists(column)
    } else {
        pending.drop_expression(column)
    };
    Ok(PyDDL {
        inner: Statement::AlterTable(ready),
    })
}

/// `ALTER TABLE ... ADD [CONSTRAINT ...] NOT NULL ... [NO INHERIT] [NOT VALID]`:
/// a not-null constraint over one column, the spelling that can leave the rows
/// already there for `validate_constraint` to check.
// [spec:pgorm:req:python.schema]
#[pyfunction]
#[pyo3(signature=(table, column, *, name=None, no_inherit=false, not_valid=false))]
pub(super) fn add_not_null(
    table: &PyTable,
    column: &Bound<'_, PyAny>,
    name: Option<&Bound<'_, PyAny>>,
    no_inherit: bool,
    not_valid: bool,
) -> PyResult<PyDDL> {
    let mut constraint = NotNullConstraint::new(PyIdentifier::new(column)?.name());
    if let Some(name) = name {
        constraint = constraint.name(PyIdentifier::new(name)?.name());
    }
    if no_inherit {
        constraint = constraint.no_inherit();
    }
    if not_valid {
        constraint = constraint.not_valid();
    }
    Ok(PyDDL {
        inner: Statement::AlterTable(Table::alter(table_name(table)?).add_not_null(constraint)),
    })
}

/// `ALTER TABLE ... VALIDATE CONSTRAINT ...`: the rows a `NOT VALID`
/// constraint skipped, checked now.
// [spec:pgorm:req:python.schema]
#[pyfunction]
pub(super) fn validate_constraint(table: &PyTable, name: &Bound<'_, PyAny>) -> PyResult<PyDDL> {
    Ok(PyDDL {
        inner: Statement::AlterTable(
            Table::alter(table_name(table)?).validate_constraint(PyIdentifier::new(name)?.name()),
        ),
    })
}

/// `ALTER TABLE ... ALTER CONSTRAINT ... <change>`, the change named as the
/// native `ConstraintChange` variant is.
// [spec:pgorm:req:python.schema]
#[pyfunction]
pub(super) fn alter_constraint(
    table: &PyTable,
    name: &Bound<'_, PyAny>,
    change: &str,
) -> PyResult<PyDDL> {
    let change = match change {
        "inherit" => ConstraintChange::Inherit,
        "no_inherit" => ConstraintChange::NoInherit,
        _ => {
            return Err(ConstructionError::new_err(
                "a constraint change is 'inherit' or 'no_inherit'",
            ));
        }
    };
    Ok(PyDDL {
        inner: Statement::AlterTable(
            Table::alter(table_name(table)?)
                .alter_constraint(PyIdentifier::new(name)?.name(), change),
        ),
    })
}
