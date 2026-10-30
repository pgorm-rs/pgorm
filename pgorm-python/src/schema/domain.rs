//! `CREATE DOMAIN`, `ALTER DOMAIN` and `DROP DOMAIN`. A domain is a type, so
//! it is named as an enum type is: one identifier, or a schema-qualified
//! `TypeName`.

use super::{
    enums::type_ref,
    statement::{PyDDL, Statement},
    types::PyDataType,
};
use crate::{
    errors::ConstructionError,
    expressions,
    expressions::Compiled,
    identifiers::{PyIdentifier, collation},
};
use pgorm::pgorm_query::{
    ConstraintDrop, Domain, DomainAlterStatement, DomainCheck, DomainCreateStatement,
    PendingDomainAlter, Values,
};
use pyo3::{prelude::*, types::PyTuple};

/// `CREATE DOMAIN`, built as `CreateTable` is: each method returns a new
/// builder carrying one more clause, rendered in the native order.
#[derive(Clone, Debug)]
#[pyclass(name = "CreateDomain", module = "pgorm.schema", frozen, from_py_object)]
pub struct PyCreateDomain {
    pub(crate) inner: DomainCreateStatement,
}

fn check(condition: &Bound<'_, PyAny>, name: Option<&Bound<'_, PyAny>>) -> PyResult<DomainCheck> {
    let check = DomainCheck::new(expressions::require_expr(condition)?.inner);
    Ok(match name {
        Some(name) => check.name(PyIdentifier::new(name)?.name()),
        None => check,
    })
}

#[pymethods]
impl PyCreateDomain {
    // [spec:pgorm:req:bindings.schema-domains]
    #[new]
    fn new(name: &Bound<'_, PyAny>, base: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: Domain::create(type_ref(name)?, PyDataType::coerce(base)?.inner),
        })
    }

    // [spec:pgorm:req:bindings.schema-domains]
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

    /// `DEFAULT`, one slot: a later call replaces an earlier one.
    fn default(&self, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut inner = self.inner.clone();
        inner.default(expressions::coerce(value)?.inner);
        Ok(Self { inner })
    }

    /// `NOT NULL`, sharing one slot with `null()`, the later call winning.
    fn not_null(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.not_null();
        Self { inner }
    }

    fn null(&self) -> Self {
        let mut inner = self.inner.clone();
        inner.null();
        Self { inner }
    }

    // [spec:pgorm:req:bindings.schema-domains]
    #[pyo3(signature=(condition, *, name=None))]
    fn check(
        &self,
        condition: &Bound<'_, PyAny>,
        name: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut inner = self.inner.clone();
        inner.check(check(condition, name)?);
        Ok(Self { inner })
    }

    pub fn inspect(&self) -> Compiled {
        Compiled {
            sql: self.inner.to_string(),
            values: Values(Vec::new()),
        }
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "SQL statements cannot be tested as Python booleans",
        ))
    }
}

/// The `ALTER DOMAIN` over the domain `domain` names whose one action
/// `action` chooses.
fn altered(
    domain: &Bound<'_, PyAny>,
    action: impl FnOnce(PendingDomainAlter) -> DomainAlterStatement,
) -> PyResult<PyDDL> {
    let pending = Domain::alter(type_ref(domain)?);
    Ok(PyDDL {
        inner: Statement::AlterDomain(action(pending)),
    })
}

// [spec:pgorm:req:bindings.schema-domains]
#[pyfunction]
pub(super) fn set_domain_default(
    domain: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<PyDDL> {
    let value = expressions::coerce(value)?.inner;
    altered(domain, |pending| pending.set_default(value))
}

#[pyfunction]
pub(super) fn drop_domain_default(domain: &Bound<'_, PyAny>) -> PyResult<PyDDL> {
    altered(domain, PendingDomainAlter::drop_default)
}

#[pyfunction]
pub(super) fn set_domain_not_null(domain: &Bound<'_, PyAny>) -> PyResult<PyDDL> {
    altered(domain, PendingDomainAlter::set_not_null)
}

#[pyfunction]
pub(super) fn drop_domain_not_null(domain: &Bound<'_, PyAny>) -> PyResult<PyDDL> {
    altered(domain, PendingDomainAlter::drop_not_null)
}

// [spec:pgorm:req:bindings.schema-domains]
#[pyfunction]
#[pyo3(signature=(domain, condition, *, name=None, not_valid=false))]
pub(super) fn add_domain_check(
    domain: &Bound<'_, PyAny>,
    condition: &Bound<'_, PyAny>,
    name: Option<&Bound<'_, PyAny>>,
    not_valid: bool,
) -> PyResult<PyDDL> {
    let check = check(condition, name)?;
    altered(domain, |pending| match not_valid {
        true => pending.add_check(check.not_valid()),
        false => pending.add_check(check),
    })
}

// [spec:pgorm:req:bindings.schema-domains]
#[pyfunction]
#[pyo3(signature=(domain, name, *, if_exists=false, cascade=false))]
pub(super) fn drop_domain_constraint(
    domain: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
    if_exists: bool,
    cascade: bool,
) -> PyResult<PyDDL> {
    let mut drop = ConstraintDrop::new(PyIdentifier::new(name)?.name());
    if if_exists {
        drop = drop.if_exists();
    }
    if cascade {
        drop = drop.cascade();
    }
    altered(domain, |pending| pending.drop_constraint(drop))
}

#[pyfunction]
pub(super) fn rename_domain_constraint(
    domain: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
    new_name: &Bound<'_, PyAny>,
) -> PyResult<PyDDL> {
    let (name, new_name) = (
        PyIdentifier::new(name)?.name(),
        PyIdentifier::new(new_name)?.name(),
    );
    altered(domain, |pending| pending.rename_constraint(name, new_name))
}

#[pyfunction]
pub(super) fn validate_domain_constraint(
    domain: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
) -> PyResult<PyDDL> {
    let name = PyIdentifier::new(name)?.name();
    altered(domain, |pending| pending.validate_constraint(name))
}

/// `RENAME TO`, the new name one identifier: a rename keeps the domain in
/// its schema.
// [spec:pgorm:req:bindings.schema-domains]
#[pyfunction]
pub(super) fn rename_domain(
    domain: &Bound<'_, PyAny>,
    new_name: &Bound<'_, PyAny>,
) -> PyResult<PyDDL> {
    let new_name = PyIdentifier::new(new_name)?.name();
    altered(domain, |pending| pending.rename_to(new_name))
}

/// `DROP DOMAIN`, over the first domain and every further positional one, so
/// the list PostgreSQL requires to be non-empty always is.
// [spec:pgorm:req:bindings.schema-domains]
#[pyfunction]
#[pyo3(signature=(domain, *domains, if_exists=false, cascade=false))]
pub(super) fn drop_domain(
    domain: &Bound<'_, PyAny>,
    domains: &Bound<'_, PyTuple>,
    if_exists: bool,
    cascade: bool,
) -> PyResult<PyDDL> {
    let mut inner = Domain::drop(type_ref(domain)?);
    for further in domains.iter() {
        inner.name(type_ref(&further)?);
    }
    if if_exists {
        inner.if_exists();
    }
    if cascade {
        inner.cascade();
    }
    Ok(PyDDL {
        inner: Statement::DropDomain(inner),
    })
}
