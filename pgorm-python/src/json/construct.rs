//! The SQL/JSON constructors. Each flag is the one spelling that differs from
//! PostgreSQL's default, as in the Rust builders: an object keeps a `NULL`
//! member unless `absent_on_null`, an array drops one unless `null_on_null`,
//! and repeated keys pass unless `unique_keys`.

use pgorm::pgorm_query::{Func, JsonArrayAgg, SimpleExpr};
use pyo3::{
    prelude::*,
    types::{PyDict, PyList, PyTuple},
};

use super::{column_type, input};
use crate::{
    errors::ConstructionError,
    expressions::{OrderBy, PyExpr, coerce},
    statements::{PySelect, condition},
};

/// `JSON_OBJECT`'s members, from a `dict` or a list or tuple of pairs. A pair's
/// key is an expression; a `dict` key is a string bound as one.
fn entries<'py>(
    value: Option<&Bound<'py, PyAny>>,
) -> PyResult<Vec<(SimpleExpr, Bound<'py, PyAny>)>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if let Ok(mapping) = value.cast_exact::<PyDict>() {
        return mapping
            .iter()
            .map(|(key, member)| Ok((coerce(&key)?.inner, member)))
            .collect();
    }
    if !value.is_exact_instance_of::<PyList>() && !value.is_exact_instance_of::<PyTuple>() {
        return Err(ConstructionError::new_err(
            "json_object requires a dict or a list or tuple of (key, value) pairs",
        ));
    }
    value
        .try_iter()?
        .map(|pair| {
            let pair = pair?;
            let (key, member) = pair
                .cast_exact::<PyTuple>()
                .ok()
                .filter(|pair| pair.len() == 2)
                .map(|pair| (pair.get_item(0), pair.get_item(1)))
                .ok_or_else(|| {
                    ConstructionError::new_err("json_object pairs are (key, value) tuples")
                })?;
            Ok((coerce(&key?)?.inner, member?))
        })
        .collect()
}

// [spec:pgorm:req:python.expressions+1]
#[pyfunction]
#[pyo3(signature = (entries=None, /, *, absent_on_null=false, unique_keys=false, returning=None))]
pub(crate) fn json_object(
    entries: Option<&Bound<'_, PyAny>>,
    absent_on_null: bool,
    unique_keys: bool,
    returning: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyExpr> {
    let mut json = Func::json_object();
    for (key, member) in self::entries(entries)? {
        json = json.entry(key, input(&member)?);
    }
    if absent_on_null {
        json = json.absent_on_null();
    }
    if unique_keys {
        json = json.with_unique_keys();
    }
    if let Some(ty) = column_type(returning)? {
        json = json.returning(ty);
    }
    Ok(PyExpr::from_rust(json.into()))
}

#[pyfunction]
#[pyo3(signature = (*elements, null_on_null=false, returning=None))]
pub(crate) fn json_array(
    elements: &Bound<'_, PyTuple>,
    null_on_null: bool,
    returning: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyExpr> {
    let mut json = Func::json_array();
    for element in elements.iter() {
        json = json.element(input(&element)?);
    }
    if null_on_null {
        json = json.null_on_null();
    }
    if let Some(ty) = column_type(returning)? {
        json = json.returning(ty);
    }
    Ok(PyExpr::from_rust(json.into()))
}

#[pyfunction]
#[pyo3(signature = (query, *, returning=None))]
pub(crate) fn json_array_query(
    query: PyRef<'_, PySelect>,
    returning: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyExpr> {
    let mut json = Func::json_array_query(query.inner.clone());
    if let Some(ty) = column_type(returning)? {
        json = json.returning(ty);
    }
    Ok(PyExpr::from_rust(json.into()))
}

#[pyfunction]
#[pyo3(signature = (
    key, value, *, absent_on_null=false, unique_keys=false, returning=None, filter=None
))]
pub(crate) fn json_objectagg(
    key: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
    absent_on_null: bool,
    unique_keys: bool,
    returning: Option<&Bound<'_, PyAny>>,
    filter: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyExpr> {
    let mut json = Func::json_objectagg(coerce(key)?.inner, input(value)?);
    if absent_on_null {
        json = json.absent_on_null();
    }
    if unique_keys {
        json = json.with_unique_keys();
    }
    if let Some(ty) = column_type(returning)? {
        json = json.returning(ty);
    }
    if let Some(filter) = filter {
        json = json.filter(condition(filter)?);
    }
    Ok(PyExpr::from_rust(json.into()))
}

/// `JSON_ARRAYAGG`'s `ORDER BY`. The Rust builder places no `NULLS FIRST` or
/// `NULLS LAST` there, so an ordering that asks for one is refused rather
/// than dropped.
fn order(mut json: JsonArrayAgg, orderings: &Bound<'_, PyAny>) -> PyResult<JsonArrayAgg> {
    if !orderings.is_exact_instance_of::<PyList>() && !orderings.is_exact_instance_of::<PyTuple>() {
        return Err(ConstructionError::new_err(
            "order_by requires a list or tuple of orderings",
        ));
    }
    for ordering in orderings.try_iter()? {
        let ordering = ordering?;
        let ordering = ordering
            .extract::<PyRef<'_, OrderBy>>()
            .map_err(|_| ConstructionError::new_err("order_by requires Expr.asc() or desc()"))?;
        if ordering.nulls.is_some() {
            return Err(ConstructionError::new_err(
                "JSON_ARRAYAGG's ORDER BY takes no NULLS FIRST or NULLS LAST",
            ));
        }
        json = json.order_by(ordering.expr.inner.clone(), ordering.direction.rust_order());
    }
    Ok(json)
}

#[pyfunction]
#[pyo3(signature = (value, *, order_by=None, null_on_null=false, returning=None, filter=None))]
pub(crate) fn json_arrayagg(
    value: &Bound<'_, PyAny>,
    order_by: Option<&Bound<'_, PyAny>>,
    null_on_null: bool,
    returning: Option<&Bound<'_, PyAny>>,
    filter: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyExpr> {
    let mut json = Func::json_arrayagg(input(value)?);
    if let Some(orderings) = order_by {
        json = order(json, orderings)?;
    }
    if null_on_null {
        json = json.null_on_null();
    }
    if let Some(ty) = column_type(returning)? {
        json = json.returning(ty);
    }
    if let Some(filter) = filter {
        json = json.filter(condition(filter)?);
    }
    Ok(PyExpr::from_rust(json.into()))
}

/// `JSON(input)`, Rust's `Func::json`, named so it cannot shadow the `json`
/// module an application imports beside `pgorm`.
#[pyfunction]
#[pyo3(signature = (input, *, unique_keys=false))]
pub(crate) fn json_parse(input: &Bound<'_, PyAny>, unique_keys: bool) -> PyResult<PyExpr> {
    let json = Func::json(super::input(input)?);
    let json = if unique_keys {
        json.with_unique_keys()
    } else {
        json
    };
    Ok(PyExpr::from_rust(json.into()))
}

#[pyfunction]
pub(crate) fn json_scalar(operand: &Bound<'_, PyAny>) -> PyResult<PyExpr> {
    Ok(PyExpr::from_rust(Func::json_scalar(coerce(operand)?.inner)))
}

#[pyfunction]
#[pyo3(signature = (input, *, returning=None))]
pub(crate) fn json_serialize(
    input: &Bound<'_, PyAny>,
    returning: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyExpr> {
    let mut json = Func::json_serialize(super::input(input)?);
    if let Some(ty) = column_type(returning)? {
        json = json.returning(ty);
    }
    Ok(PyExpr::from_rust(json.into()))
}
