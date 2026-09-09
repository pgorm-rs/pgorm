use pyo3::{
    prelude::*,
    types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple},
};
use serde_json::{Map, Number, Value};

use crate::errors::ConstructionError;

pub(super) fn from_python(value: &Bound<'_, PyAny>, depth: usize) -> PyResult<Value> {
    if depth > 64 {
        return Err(ConstructionError::new_err(
            "JSON exceeds 64 levels or contains a cycle",
        ));
    }
    if value.is_none() {
        return Ok(Value::Null);
    }
    if value.is_exact_instance_of::<PyBool>() {
        return Ok(Value::Bool(value.extract()?));
    }
    if value.is_exact_instance_of::<PyInt>() {
        return if let Ok(number) = value.extract::<i64>() {
            Ok(Value::Number(number.into()))
        } else if let Ok(number) = value.extract::<u64>() {
            Ok(Value::Number(number.into()))
        } else {
            Err(ConstructionError::new_err(
                "JSON integer exceeds the Rust i64/u64 range",
            ))
        };
    }
    if value.is_exact_instance_of::<PyFloat>() {
        return Number::from_f64(value.extract()?)
            .map(Value::Number)
            .ok_or_else(|| ConstructionError::new_err("JSON does not support non-finite floats"));
    }
    if value.is_exact_instance_of::<PyString>() {
        return Ok(Value::String(value.extract()?));
    }
    if value.is_exact_instance_of::<PyList>() || value.is_exact_instance_of::<PyTuple>() {
        return Ok(Value::Array(
            value
                .try_iter()?
                .map(|item| from_python(&item?, depth + 1))
                .collect::<PyResult<_>>()?,
        ));
    }
    if value.is_exact_instance_of::<PyDict>() {
        let mut object = Map::new();
        for (key, item) in value.cast::<PyDict>()?.iter() {
            if !key.is_exact_instance_of::<PyString>() {
                return Err(ConstructionError::new_err(
                    "JSON object keys must be strings",
                ));
            }
            object.insert(key.extract()?, from_python(&item, depth + 1)?);
        }
        return Ok(Value::Object(object));
    }
    Err(ConstructionError::new_err("unsupported JSON value type"))
}

pub(super) fn to_python(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    Ok(py
        .import("json")?
        .call_method1("loads", (value.to_string(),))?
        .unbind())
}
