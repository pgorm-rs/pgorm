//! `CREATE SCHEMA`, `DROP SCHEMA` and `ALTER SCHEMA ... RENAME TO`: a schema
//! is named by one identifier, never a qualified `Table`, schemas not
//! nesting.

use super::{
    args,
    drops::drop_list,
    statement::{PyDDL, Statement},
};
use pgorm::pgorm_query::Schema;
use pyo3::prelude::*;

// [spec:pgorm:req:bindings.schema-schemas]
#[pyfunction]
#[pyo3(signature=(name, *, if_not_exists=false))]
pub(super) fn create_schema(name: &Bound<'_, PyAny>, if_not_exists: bool) -> PyResult<PyDDL> {
    let mut create = Schema::create(args::name(name)?);
    if if_not_exists {
        create.if_not_exists();
    }
    Ok(PyDDL::new(Statement::CreateSchema(create)))
}

/// `DROP SCHEMA` over one schema or a non-empty list of them, in order.
// [spec:pgorm:req:bindings.schema-schemas]
#[pyfunction]
#[pyo3(signature=(names, *, if_exists=false, behavior=None))]
pub(super) fn drop_schema(
    names: &Bound<'_, PyAny>,
    if_exists: bool,
    behavior: Option<&str>,
) -> PyResult<PyDDL> {
    let drop = drop_list(args::columns(names, "a drop")?, if_exists, behavior)?;
    Ok(PyDDL::new(Statement::DropSchema(drop)))
}

/// `ALTER SCHEMA ... RENAME TO`, which PostgreSQL gives no `IF EXISTS`.
// [spec:pgorm:req:bindings.schema-schemas]
#[pyfunction]
pub(super) fn rename_schema(
    name: &Bound<'_, PyAny>,
    new_name: &Bound<'_, PyAny>,
) -> PyResult<PyDDL> {
    let renamed = Schema::rename(args::name(name)?, args::name(new_name)?);
    Ok(PyDDL::new(Statement::RenameSchema(renamed)))
}
