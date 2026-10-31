//! `CREATE TABLE` with its columns, keys, foreign keys and `CHECK`s; the
//! statements that drop, rename and empty a table; and comments.

use super::{
    args::{self, relation, relations, text},
    column::{self, PyColumnDef},
    drops::drop_list,
    keys::{self, ForeignKeyOptions},
    statement::{PyDDL, Statement, ddl_class},
};
use pgorm::pgorm_query::{
    Comment, ConstraintRenameStatement, Primary, Table, TableCreateStatement, Unique,
};
use pyo3::prelude::*;

/// `CREATE TABLE`; every method gives a new statement.
#[derive(Debug)]
#[pyclass(name = "CreateTable", module = "pgorm.schema", frozen, extends = PyDDL)]
pub struct PyCreateTable;

ddl_class!(PyCreateTable, CreateTable, TableCreateStatement, with);

// [spec:pgorm:req:bindings.schema-tables]
#[pyfunction]
pub(super) fn create_table<'py>(
    py: Python<'py>,
    table: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyCreateTable>> {
    PyCreateTable::object(py, Table::create(relation(table)?))
}

#[pymethods]
impl PyCreateTable {
    /// A column, which needs its type.
    fn column<'py>(slf: &Bound<'py, Self>, column: &PyColumnDef) -> PyResult<Bound<'py, Self>> {
        let column = column.typed("a column a table is created with")?;
        Self::with(slf, |statement| {
            statement.col(column);
        })
    }

    fn if_not_exists<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, Self>> {
        let mut statement = Self::statement(slf)?;
        statement.if_not_exists();
        Self::object(slf.py(), statement)
    }

    /// The table's one primary key: a later call replaces the key an earlier
    /// one declared, as the native builder's one slot does.
    // [spec:pgorm:req:bindings.schema-tables]
    #[pyo3(signature=(columns, *, name=None, include=None, deferrability=None, without_overlaps=None))]
    fn primary_key<'py>(
        slf: &Bound<'py, Self>,
        columns: &Bound<'_, PyAny>,
        name: Option<&Bound<'_, PyAny>>,
        include: Option<&Bound<'_, PyAny>>,
        deferrability: Option<&str>,
        without_overlaps: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, Self>> {
        let key = keys::key::<Primary>(columns, name, include, deferrability, without_overlaps)?;
        Self::with(slf, |statement| {
            statement.primary_key(key);
        })
    }

    #[pyo3(signature=(columns, *, name=None, include=None, deferrability=None, without_overlaps=None, nulls_not_distinct=false))]
    #[allow(clippy::too_many_arguments)]
    fn unique<'py>(
        slf: &Bound<'py, Self>,
        columns: &Bound<'_, PyAny>,
        name: Option<&Bound<'_, PyAny>>,
        include: Option<&Bound<'_, PyAny>>,
        deferrability: Option<&str>,
        without_overlaps: Option<&Bound<'_, PyAny>>,
        nulls_not_distinct: bool,
    ) -> PyResult<Bound<'py, Self>> {
        let key = keys::key::<Unique>(columns, name, include, deferrability, without_overlaps)?;
        let key = keys::unique(key, nulls_not_distinct);
        Self::with(slf, |statement| {
            statement.unique(key);
        })
    }

    /// Columns paired in order with as many columns of the table they
    /// reference.
    // [spec:pgorm:req:bindings.schema-tables]
    #[pyo3(signature=(columns, references, ref_columns, *, name=None, on_delete=None, on_update=None, deferrability=None, enforcement=None, period=None))]
    #[allow(clippy::too_many_arguments)]
    fn foreign_key<'py>(
        slf: &Bound<'py, Self>,
        columns: &Bound<'_, PyAny>,
        references: &Bound<'_, PyAny>,
        ref_columns: &Bound<'_, PyAny>,
        name: Option<&Bound<'_, PyAny>>,
        on_delete: Option<&str>,
        on_update: Option<&str>,
        deferrability: Option<&str>,
        enforcement: Option<&str>,
        period: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, Self>> {
        let table = Self::statement(slf)?.get_table_name().clone();
        let options = ForeignKeyOptions {
            name,
            on_delete,
            on_update,
            deferrability,
            enforcement,
            period,
        };
        let key = keys::foreign_key(table, columns, references, ref_columns, options)?;
        Self::with(slf, |statement| {
            statement.foreign_key(key);
        })
    }

    #[pyo3(signature=(condition, *, name=None, enforcement=None, no_inherit=false))]
    fn check<'py>(
        slf: &Bound<'py, Self>,
        condition: &Bound<'_, PyAny>,
        name: Option<&Bound<'_, PyAny>>,
        enforcement: Option<&str>,
        no_inherit: bool,
    ) -> PyResult<Bound<'py, Self>> {
        let check = column::check(condition, name, enforcement, no_inherit)?;
        Self::with(slf, |statement| {
            statement.check(check);
        })
    }
}

/// `DROP TABLE` over one table or a non-empty list of them.
// [spec:pgorm:req:bindings.schema-tables]
#[pyfunction]
#[pyo3(signature=(tables, *, if_exists=false, behavior=None))]
pub(super) fn drop_table(
    tables: &Bound<'_, PyAny>,
    if_exists: bool,
    behavior: Option<&str>,
) -> PyResult<PyDDL> {
    let drop = drop_list(relations(tables)?, if_exists, behavior)?;
    Ok(PyDDL::new(Statement::DropTable(drop)))
}

/// `RENAME TO`: the new name is bare, a rename leaving the table in its
/// schema.
#[pyfunction]
pub(super) fn rename_table(table: &Bound<'_, PyAny>, name: &Bound<'_, PyAny>) -> PyResult<PyDDL> {
    let renamed = Table::rename(relation(table)?, args::name(name)?);
    Ok(PyDDL::new(Statement::RenameTable(renamed)))
}

#[pyfunction]
pub(super) fn rename_column(
    table: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
    new_name: &Bound<'_, PyAny>,
) -> PyResult<PyDDL> {
    let renamed = Table::rename_column(relation(table)?, args::name(name)?, args::name(new_name)?);
    Ok(PyDDL::new(Statement::RenameColumn(renamed)))
}

/// `ALTER TABLE ... RENAME CONSTRAINT ... TO ...`: a constraint of any kind,
/// renamed by a statement of its own, as a column is.
// [spec:pgorm:req:bindings.schema-tables]
#[pyfunction]
pub(super) fn rename_constraint(
    table: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
    new_name: &Bound<'_, PyAny>,
) -> PyResult<PyDDL> {
    let renamed =
        ConstraintRenameStatement::new(relation(table)?, args::name(name)?, args::name(new_name)?);
    Ok(PyDDL::new(Statement::RenameConstraint(renamed)))
}

#[pyfunction]
pub(super) fn truncate_table(table: &Bound<'_, PyAny>) -> PyResult<PyDDL> {
    let truncate = Table::truncate(relation(table)?);
    Ok(PyDDL::new(Statement::Truncate(truncate)))
}

/// `COMMENT ON TABLE`: the text a literal pgorm-query escapes.
// [spec:pgorm:req:bindings.schema-sequences]
#[pyfunction]
pub(super) fn comment_on_table(
    table: &Bound<'_, PyAny>,
    text: &Bound<'_, PyAny>,
) -> PyResult<PyDDL> {
    let comment = Comment::on_table(relation(table)?, self::text(text, "a comment")?);
    Ok(PyDDL::new(Statement::Comment(comment)))
}

#[pyfunction]
pub(super) fn comment_on_column(
    table: &Bound<'_, PyAny>,
    column: &Bound<'_, PyAny>,
    text: &Bound<'_, PyAny>,
) -> PyResult<PyDDL> {
    let comment = Comment::on_column(
        relation(table)?,
        args::name(column)?,
        self::text(text, "a comment")?,
    );
    Ok(PyDDL::new(Statement::Comment(comment)))
}
