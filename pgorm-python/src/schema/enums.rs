use super::statement::{PyDDL, Statement};
use crate::{errors::ConstructionError, identifiers::PyIdentifier, values::PyTypeName};
use pgorm::pgorm_query::{
    Alias, IntoIden,
    extension::{Type, TypeRef},
};
use pyo3::prelude::*;

fn type_ref(name: &Bound<'_, PyAny>) -> PyResult<TypeRef> {
    if let Ok(name) = name.extract::<PyRef<'_, PyTypeName>>() {
        let local = Alias::new(&name.name).into_iden();
        return Ok(match &name.schema {
            Some(schema) => TypeRef::SchemaType(Alias::new(schema).into_iden(), local),
            None => TypeRef::Type(local),
        });
    }
    Ok(TypeRef::Type(PyIdentifier::new(name)?.alias().into_iden()))
}

fn label(value: &Bound<'_, PyAny>) -> PyResult<Alias> {
    let text = value
        .extract::<&str>()
        .map_err(|_| ConstructionError::new_err("enum labels require strings"))?;
    if text.len() > 63 || text.contains('\0') {
        return Err(ConstructionError::new_err(
            "enum labels require at most 63 UTF-8 bytes and no NUL",
        ));
    }
    Ok(Alias::new(text))
}

// [spec:pgorm:req:python.schema]
#[pyfunction]
pub(super) fn create_enum(
    name: &Bound<'_, PyAny>,
    values: Vec<Bound<'_, PyAny>>,
) -> PyResult<PyDDL> {
    let labels = values.iter().map(label).collect::<PyResult<Vec<_>>>()?;
    let mut inner = Type::create(type_ref(name)?);
    inner.as_enum().values(labels);
    Ok(PyDDL {
        inner: Statement::CreateEnum(inner),
    })
}

#[pyfunction]
#[pyo3(signature=(name, value, *, before=None, after=None))]
pub(super) fn add_enum_value(
    name: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
    before: Option<&Bound<'_, PyAny>>,
    after: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyDDL> {
    if before.is_some() && after.is_some() {
        return Err(ConstructionError::new_err("choose either before or after"));
    }
    let mut inner = Type::alter(type_ref(name)?).add_value(label(value)?);
    if let Some(before) = before {
        inner = inner.before(label(before)?);
    }
    if let Some(after) = after {
        inner = inner.after(label(after)?);
    }
    Ok(PyDDL {
        inner: Statement::AlterEnum(inner),
    })
}

#[pyfunction]
pub(super) fn rename_enum_value(
    name: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
    new_value: &Bound<'_, PyAny>,
) -> PyResult<PyDDL> {
    Ok(PyDDL {
        inner: Statement::AlterEnum(
            Type::alter(type_ref(name)?).rename_value(label(value)?, label(new_value)?),
        ),
    })
}

#[pyfunction]
pub(super) fn rename_enum(name: &Bound<'_, PyAny>, new_name: &Bound<'_, PyAny>) -> PyResult<PyDDL> {
    Ok(PyDDL {
        inner: Statement::AlterEnum(
            Type::alter(type_ref(name)?).rename_to(PyIdentifier::new(new_name)?.alias()),
        ),
    })
}

#[pyfunction]
#[pyo3(signature=(name, *, if_exists=false, cascade=false))]
pub(super) fn drop_enum(
    name: &Bound<'_, PyAny>,
    if_exists: bool,
    cascade: bool,
) -> PyResult<PyDDL> {
    let mut inner = Type::drop(type_ref(name)?);
    if if_exists {
        inner.if_exists();
    }
    if cascade {
        inner.cascade();
    }
    Ok(PyDDL {
        inner: Statement::DropEnum(inner),
    })
}
