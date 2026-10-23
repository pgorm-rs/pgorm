//! Views, materialized views and `CREATE TABLE AS`: a named relation over a
//! `Select`, whose values the Rust renderer writes into the definition as
//! literals, a definition being stored as SQL text that binds nothing.
//!
//! Nothing can order a query or declare its windows once it is a definition,
//! so a tie limit or a WINDOW clause PostgreSQL would refuse is refused as
//! the function is called. The Rust constructor runs the two checks
//! `PySelect::embedded` runs, in the same order and with the same errors, on
//! the query it takes, so the query is handed over unchecked and the
//! constructor's refusal is raised as the `ConstructionError` an embedded
//! read would raise.

use super::{
    statement::{PyDDL, Statement},
    table::table_name,
};
use crate::{
    errors::ConstructionError,
    identifiers::PyIdentifier,
    statements::{PySelect, PyTable},
};
use pgorm::pgorm_query::{MaterializedView, Name, Table, View, ViewCheckOption};
use pyo3::prelude::*;

fn column_names(columns: &[Bound<'_, PyAny>]) -> PyResult<Vec<Name>> {
    columns
        .iter()
        .map(|column| Ok(PyIdentifier::new(column)?.name()))
        .collect()
}

/// `"local"` or `"cascaded"`, the two check options; the bare `WITH CHECK
/// OPTION` means `CASCADED` and so has no spelling of its own.
fn check_option(option: &str) -> PyResult<ViewCheckOption> {
    match option {
        "local" => Ok(ViewCheckOption::Local),
        "cascaded" => Ok(ViewCheckOption::Cascaded),
        _ => Err(ConstructionError::new_err(
            "a view's check option is \"local\" or \"cascaded\"",
        )),
    }
}

/// `CREATE [OR REPLACE] VIEW`: the check option exists only here, the
/// recursive form having none, as PostgreSQL's grammar refuses one there.
// [spec:pgorm:req:python.schema-views+2]
#[pyfunction]
#[pyo3(signature=(name, query, *, columns=Vec::new(), or_replace=false, check_option=None, security_barrier=false, security_invoker=false))]
#[allow(clippy::too_many_arguments)]
pub(super) fn create_view(
    name: &PyTable,
    query: PyRef<'_, PySelect>,
    columns: Vec<Bound<'_, PyAny>>,
    or_replace: bool,
    check_option: Option<&str>,
    security_barrier: bool,
    security_invoker: bool,
) -> PyResult<PyDDL> {
    let option = check_option.map(self::check_option).transpose()?;
    let mut inner = View::create(table_name(name)?, query.inner.clone())
        .map_err(|error| ConstructionError::new_err(error.to_string()))?;
    inner.columns(column_names(&columns)?);
    if or_replace {
        inner.or_replace();
    }
    if let Some(option) = option {
        inner.check_option(option);
    }
    if security_barrier {
        inner.security_barrier();
    }
    if security_invoker {
        inner.security_invoker();
    }
    Ok(PyDDL {
        inner: Statement::CreateView(inner),
    })
}

/// `CREATE [OR REPLACE] RECURSIVE VIEW`: its column list names the rows of
/// the `WITH RECURSIVE` query PostgreSQL defines it as, and the grammar
/// refuses one without, so an empty list is refused here.
// [spec:pgorm:req:python.schema-views+2]
#[pyfunction]
#[pyo3(signature=(name, columns, query, *, or_replace=false, security_barrier=false, security_invoker=false))]
pub(super) fn create_recursive_view(
    name: &PyTable,
    columns: Vec<Bound<'_, PyAny>>,
    query: PyRef<'_, PySelect>,
    or_replace: bool,
    security_barrier: bool,
    security_invoker: bool,
) -> PyResult<PyDDL> {
    let mut columns = column_names(&columns)?.into_iter();
    let Some(first) = columns.next() else {
        return Err(ConstructionError::new_err(
            "a recursive view names at least one column",
        ));
    };
    let mut inner = View::create_recursive(table_name(name)?, first, query.inner.clone())
        .map_err(|error| ConstructionError::new_err(error.to_string()))?;
    inner.columns(columns);
    if or_replace {
        inner.or_replace();
    }
    if security_barrier {
        inner.security_barrier();
    }
    if security_invoker {
        inner.security_invoker();
    }
    Ok(PyDDL {
        inner: Statement::CreateRecursiveView(inner),
    })
}

// [spec:pgorm:req:python.schema-views+2]
#[pyfunction]
#[pyo3(signature=(name, *, if_exists=false, cascade=false))]
pub(super) fn drop_view(name: &PyTable, if_exists: bool, cascade: bool) -> PyResult<PyDDL> {
    let mut inner = View::drop(table_name(name)?);
    if if_exists {
        inner.if_exists();
    }
    if cascade {
        inner.cascade();
    }
    Ok(PyDDL {
        inner: Statement::DropView(inner),
    })
}

// [spec:pgorm:req:python.schema-views+2]
#[pyfunction]
#[pyo3(signature=(name, query, *, columns=Vec::new(), if_not_exists=false, with_no_data=false))]
pub(super) fn create_materialized_view(
    name: &PyTable,
    query: PyRef<'_, PySelect>,
    columns: Vec<Bound<'_, PyAny>>,
    if_not_exists: bool,
    with_no_data: bool,
) -> PyResult<PyDDL> {
    let mut inner = MaterializedView::create(table_name(name)?, query.inner.clone())
        .map_err(|error| ConstructionError::new_err(error.to_string()))?;
    inner.columns(column_names(&columns)?);
    if if_not_exists {
        inner.if_not_exists();
    }
    if with_no_data {
        inner.with_no_data();
    }
    Ok(PyDDL {
        inner: Statement::CreateMaterializedView(inner),
    })
}

/// `REFRESH MATERIALIZED VIEW`. PostgreSQL refuses `CONCURRENTLY` beside
/// `WITH NO DATA`, and the Rust statement holds the two in one slot, so both
/// together are refused here rather than one silently kept.
// [spec:pgorm:req:python.schema-views+2]
#[pyfunction]
#[pyo3(signature=(name, *, concurrently=false, with_no_data=false))]
pub(super) fn refresh_materialized_view(
    name: &PyTable,
    concurrently: bool,
    with_no_data: bool,
) -> PyResult<PyDDL> {
    if concurrently && with_no_data {
        return Err(ConstructionError::new_err(
            "a refresh is concurrent or with no data, never both",
        ));
    }
    let mut inner = MaterializedView::refresh(table_name(name)?);
    if concurrently {
        inner.concurrently();
    }
    if with_no_data {
        inner.with_no_data();
    }
    Ok(PyDDL {
        inner: Statement::RefreshMaterializedView(inner),
    })
}

// [spec:pgorm:req:python.schema-views+2]
#[pyfunction]
#[pyo3(signature=(name, *, if_exists=false, cascade=false))]
pub(super) fn drop_materialized_view(
    name: &PyTable,
    if_exists: bool,
    cascade: bool,
) -> PyResult<PyDDL> {
    let mut inner = MaterializedView::drop(table_name(name)?);
    if if_exists {
        inner.if_exists();
    }
    if cascade {
        inner.cascade();
    }
    Ok(PyDDL {
        inner: Statement::DropMaterializedView(inner),
    })
}

// [spec:pgorm:req:python.schema-views+2]
#[pyfunction]
#[pyo3(signature=(name, query, *, columns=Vec::new(), if_not_exists=false, with_no_data=false))]
pub(super) fn create_table_as(
    name: &PyTable,
    query: PyRef<'_, PySelect>,
    columns: Vec<Bound<'_, PyAny>>,
    if_not_exists: bool,
    with_no_data: bool,
) -> PyResult<PyDDL> {
    let mut inner = Table::create_as(table_name(name)?, query.inner.clone())
        .map_err(|error| ConstructionError::new_err(error.to_string()))?;
    inner.columns(column_names(&columns)?);
    if if_not_exists {
        inner.if_not_exists();
    }
    if with_no_data {
        inner.with_no_data();
    }
    Ok(PyDDL {
        inner: Statement::CreateTableAs(inner),
    })
}
