use super::types::PyDataType;
use crate::{
    errors::ConstructionError,
    expressions,
    identifiers::{PyIdentifier, collation},
};
use pgorm::pgorm_query::{Check, ColumnDef, Enforcement, GeneratedKind};
use pyo3::{exceptions::PyTypeError, prelude::*};

#[derive(Clone, Debug)]
#[pyclass(name = "ColumnDef", module = "pgorm.schema", frozen, from_py_object)]
pub struct PyColumnDef {
    pub(crate) inner: ColumnDef,
}

#[pymethods]
impl PyColumnDef {
    // [spec:pgorm:req:bindings.schema-tables]
    #[new]
    fn new(name: &Bound<'_, PyAny>, kind: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: ColumnDef::new_with_type(
                PyIdentifier::new(name)?.name(),
                PyDataType::coerce(kind)?.inner,
            ),
        })
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.get_column_name()
    }

    /// `[CONSTRAINT "name" ]NOT NULL[ NO INHERIT]`: the column's one not-null
    /// constraint, named and kept from inheriting tables as the native
    /// `not_null_named` and `not_null_no_inherit` set it.
    // [spec:pgorm:req:bindings.schema-tables]
    #[pyo3(signature=(*, name=None, no_inherit=false))]
    fn not_null(&self, name: Option<&Bound<'_, PyAny>>, no_inherit: bool) -> PyResult<Self> {
        let mut inner = self.inner.clone();
        inner.not_null();
        if let Some(name) = name {
            inner.not_null_named(PyIdentifier::new(name)?.name());
        }
        if no_inherit {
            inner.not_null_no_inherit();
        }
        Ok(Self { inner })
    }
    fn null(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.null();
        Self { inner }
    }
    fn auto_increment(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.auto_increment();
        Self { inner }
    }
    fn default(&self, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut inner = self.inner.clone();
        inner.default(expressions::coerce(value)?.inner);
        Ok(Self { inner })
    }
    // [spec:pgorm:req:bindings.schema-tables]
    #[pyo3(signature=(condition, *, name=None, not_enforced=false, no_inherit=false))]
    fn check(
        &self,
        condition: &Bound<'_, PyAny>,
        name: Option<&Bound<'_, PyAny>>,
        not_enforced: bool,
        no_inherit: bool,
    ) -> PyResult<Self> {
        let mut inner = self.inner.clone();
        inner.check(check(condition, name, not_enforced, no_inherit)?);
        Ok(Self { inner })
    }
    /// `COLLATE` after the type: the column's one collation, a later call
    /// replacing an earlier, since PostgreSQL refuses a column with two.
    /// `modify_column` writes it on the retype, the only place PostgreSQL
    /// changes a column's collation.
    // [spec:pgorm:req:bindings.collations]
    #[pyo3(signature=(name, *, schema=None))]
    fn collate(
        &self,
        name: &Bound<'_, PyAny>,
        schema: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut inner = self.inner.clone();
        inner.collate(collation(name, schema)?);
        Ok(Self { inner })
    }
    fn generated(&self, expression: &Bound<'_, PyAny>, kind: &str) -> PyResult<Self> {
        let kind = match kind {
            "stored" => GeneratedKind::Stored,
            "virtual" => GeneratedKind::Virtual,
            _ => {
                return Err(PyTypeError::new_err(
                    "a generated column is 'stored' or 'virtual'",
                ));
            }
        };
        let mut inner = self.inner.clone();
        inner.generated(expressions::require_expr(expression)?.inner, kind);
        Ok(Self { inner })
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "column definitions cannot be tested as Python booleans",
        ))
    }
}

/// A `CHECK` over `condition`, named, `NOT ENFORCED` and `NO INHERIT` as
/// asked: what `ColumnDef.check`, `CreateTable.check` and `add_check` each
/// build.
// [spec:pgorm:req:bindings.schema-tables]
pub(super) fn check(
    condition: &Bound<'_, PyAny>,
    name: Option<&Bound<'_, PyAny>>,
    not_enforced: bool,
    no_inherit: bool,
) -> PyResult<Check> {
    let mut check = Check::new(expressions::require_expr(condition)?.inner);
    if let Some(name) = name {
        check = check.name(PyIdentifier::new(name)?.name());
    }
    if no_inherit {
        check = check.no_inherit();
    }
    if not_enforced {
        check = check.enforcement(Enforcement::NotEnforced);
    }
    Ok(check)
}
