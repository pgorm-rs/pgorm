//! `CREATE SCHEMA`, `DROP SCHEMA` and `ALTER SCHEMA ... RENAME TO`: a schema
//! is named by one identifier, never a qualified `Table`, schemas not
//! nesting.

use super::statement::{PyDDL, Statement};
use crate::identifiers::PyIdentifier;
use pgorm::pgorm_query::Schema;
use pyo3::{prelude::*, types::PyTuple};

// [spec:pgorm:req:bindings.schema-schemas]
#[pyfunction]
#[pyo3(signature=(name, *, if_not_exists=false))]
pub(super) fn create_schema(name: &Bound<'_, PyAny>, if_not_exists: bool) -> PyResult<PyDDL> {
    let mut inner = Schema::create(PyIdentifier::new(name)?.name());
    if if_not_exists {
        inner.if_not_exists();
    }
    Ok(PyDDL {
        inner: Statement::CreateSchema(inner),
    })
}

/// `DROP SCHEMA`, over the first name and every further positional one, so
/// the list PostgreSQL requires to be non-empty always is.
// [spec:pgorm:req:bindings.schema-schemas]
#[pyfunction]
#[pyo3(signature=(name, *names, if_exists=false, cascade=false))]
pub(super) fn drop_schema(
    name: &Bound<'_, PyAny>,
    names: &Bound<'_, PyTuple>,
    if_exists: bool,
    cascade: bool,
) -> PyResult<PyDDL> {
    let mut inner = Schema::drop(PyIdentifier::new(name)?.name());
    for further in names.iter() {
        inner.name(PyIdentifier::new(&further)?.name());
    }
    if if_exists {
        inner.if_exists();
    }
    if cascade {
        inner.cascade();
    }
    Ok(PyDDL {
        inner: Statement::DropSchema(inner),
    })
}

/// `ALTER SCHEMA ... RENAME TO`, which PostgreSQL gives no `IF EXISTS`.
// [spec:pgorm:req:bindings.schema-schemas]
#[pyfunction]
pub(super) fn rename_schema(
    name: &Bound<'_, PyAny>,
    new_name: &Bound<'_, PyAny>,
) -> PyResult<PyDDL> {
    Ok(PyDDL {
        inner: Statement::RenameSchema(Schema::rename(
            PyIdentifier::new(name)?.name(),
            PyIdentifier::new(new_name)?.name(),
        )),
    })
}
