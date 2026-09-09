use pgorm::pgorm_query::{ColumnRef, Condition, Expr, Func, IntoIden, Query, SimpleExpr};
use pyo3::{prelude::*, types::PyTuple};

use super::{PyExpr, compiled::Compiled};
use crate::{errors::ConstructionError, identifiers::PyIdentifier, values::PyValue};

// [spec:pgorm:req:python.delegation+1]
// [spec:pgorm:req:python.input-boundaries]
#[pyfunction]
#[pyo3(signature = (name, *, table=None, schema=None))]
pub(crate) fn col(
    name: &Bound<'_, PyAny>,
    table: Option<&Bound<'_, PyAny>>,
    schema: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyExpr> {
    let name = PyIdentifier::new(name)?.alias().into_iden();
    let table = table.map(PyIdentifier::new).transpose()?;
    let schema = schema.map(PyIdentifier::new).transpose()?;
    let column = match (schema, table) {
        (Some(schema), Some(table)) => ColumnRef::SchemaTableColumn(
            schema.alias().into_iden(),
            table.alias().into_iden(),
            name,
        ),
        (None, Some(table)) => ColumnRef::TableColumn(table.alias().into_iden(), name),
        (None, None) => ColumnRef::Column(name),
        (Some(_), None) => {
            return Err(ConstructionError::new_err(
                "a schema-qualified column requires a table",
            ));
        }
    };
    Ok(PyExpr::from_rust(Expr::col(column).into()))
}

#[pyfunction]
pub(crate) fn bind(value: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
    value_expr(value, false)
}

#[pyfunction]
pub(crate) fn literal(value: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
    value_expr(value, true)
}

fn value_expr(value: &Bound<'_, PyAny>, literal: bool) -> PyResult<PyExpr> {
    let value = PyValue::coerce(value)?;
    let mut inner = if literal {
        SimpleExpr::Constant(value.rust_value().clone())
    } else {
        Expr::value(value.rust_value().clone())
    };
    if let Some(cast) = value.enum_cast() {
        inner = inner.cast_as_type(cast);
    }
    Ok(PyExpr::from_rust(inner))
}

pub(crate) fn coerce(value: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
    if let Ok(expr) = value.extract::<PyRef<'_, PyExpr>>() {
        Ok(expr.clone())
    } else {
        bind(value)
    }
}

pub(super) fn sequence(values: &Bound<'_, PyAny>) -> PyResult<Vec<SimpleExpr>> {
    if !values.is_exact_instance_of::<pyo3::types::PyList>()
        && !values.is_exact_instance_of::<PyTuple>()
    {
        return Err(ConstructionError::new_err(
            "expression sequence requires a list or tuple",
        ));
    }
    values
        .try_iter()?
        .map(|value| coerce(&value?).map(|expr| expr.inner))
        .collect()
}

#[pyfunction]
#[pyo3(signature = (*expressions))]
pub(crate) fn tuple_expr(expressions: &Bound<'_, PyTuple>) -> PyResult<PyExpr> {
    if expressions.is_empty() {
        return Err(ConstructionError::new_err(
            "tuple expressions require at least one item",
        ));
    }
    Ok(PyExpr::from_rust(
        Expr::tuple(sequence(expressions.as_any())?).into(),
    ))
}

/// A reusable Rust condition; empty All/Any preserve the builder's identities.
#[pyclass(name = "Condition", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyCondition {
    pub inner: Condition,
}

#[pymethods]
impl PyCondition {
    #[staticmethod]
    #[pyo3(signature = (*conditions))]
    fn all(conditions: &Bound<'_, PyTuple>) -> PyResult<Self> {
        condition(Condition::all(), conditions)
    }

    #[staticmethod]
    #[pyo3(signature = (*conditions))]
    fn any(conditions: &Bound<'_, PyTuple>) -> PyResult<Self> {
        condition(Condition::any(), conditions)
    }

    fn add(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        let inner = if let Ok(other) = other.extract::<PyRef<'_, Self>>() {
            self.inner.clone().add(other.inner.clone())
        } else {
            self.inner.clone().add(require_expr(other)?.inner)
        };
        Ok(Self { inner })
    }

    fn __invert__(&self) -> Self {
        Self {
            inner: self.inner.clone().not(),
        }
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "SQL conditions cannot be tested as Python booleans",
        ))
    }

    /// Inspect the condition in the WHERE clause of a Rust-built SELECT TRUE.
    fn inspect(&self) -> Compiled {
        let (sql, values) = Query::select()
            .expr(SimpleExpr::Constant(true.into()))
            .cond_where(self.inner.clone())
            .build();
        Compiled { sql, values }
    }

    fn __repr__(&self) -> String {
        format!("Condition(terms={})", self.inner.len())
    }
}

fn condition(mut inner: Condition, values: &Bound<'_, PyTuple>) -> PyResult<PyCondition> {
    for value in values.iter() {
        inner = if let Ok(condition) = value.extract::<PyRef<'_, PyCondition>>() {
            inner.add(condition.inner.clone())
        } else {
            inner.add(require_expr(&value)?.inner)
        };
    }
    Ok(PyCondition { inner })
}

pub(crate) fn require_expr(value: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
    value
        .extract::<PyRef<'_, PyExpr>>()
        .map(|value| value.clone())
        .map_err(|_| ConstructionError::new_err("expected a query expression"))
}

// [spec:pgorm:req:python.expressions]
#[pyfunction]
#[pyo3(signature = (name, *arguments))]
pub(crate) fn call(name: &str, arguments: &Bound<'_, PyTuple>) -> PyResult<PyExpr> {
    let args = sequence(arguments.as_any())?;
    let expr = match (name, args.as_slice()) {
        ("lower", [value]) => Func::lower(value.clone()),
        ("upper", [value]) => Func::upper(value.clone()),
        ("abs", [value]) => Func::abs(value.clone()),
        ("char_length", [value]) => Func::char_length(value.clone()),
        ("count", [value]) => Func::count(value.clone()),
        ("count_distinct", [value]) => Func::count_distinct(value.clone()),
        ("sum", [value]) => Func::sum(value.clone()),
        ("avg", [value]) => Func::avg(value.clone()),
        ("min", [value]) => Func::min(value.clone()),
        ("max", [value]) => Func::max(value.clone()),
        ("round", [value]) => Func::round(value.clone()),
        ("round", [value, precision]) => {
            Func::round_with_precision(value.clone(), precision.clone())
        }
        ("coalesce", [_, ..]) => Func::coalesce(args),
        ("random", []) => Func::random(),
        ("gen_random_uuid", []) => Func::gen_random_uuid(),
        _ => {
            return Err(crate::UnsupportedCapabilityError::new_err(
                "unsupported function or argument count",
            ));
        }
    };
    Ok(PyExpr::from_rust(expr.into()))
}
