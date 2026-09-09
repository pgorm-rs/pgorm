use pgorm::pgorm_query::{ArrayType, IpNetwork, MacAddress, Value, Vector};
use pyo3::IntoPyObjectExt;
use pyo3::{
    prelude::*,
    types::{
        PyBool, PyBytes, PyDate, PyDateTime, PyFloat, PyInt, PyList, PyString, PyTime, PyTuple,
    },
};
use rust_decimal::Decimal;
use uuid::Uuid;

use super::{json, temporal};
use crate::errors::{ConstructionError, DecodeError};

pub(super) fn infer(data: &Bound<'_, PyAny>) -> PyResult<ArrayType> {
    let kind = if data.is_exact_instance_of::<PyBool>() {
        ArrayType::Bool
    } else if data.is_exact_instance_of::<PyInt>() {
        ArrayType::BigInt
    } else if data.is_exact_instance_of::<PyFloat>() {
        ArrayType::Double
    } else if data.is_exact_instance_of::<PyString>() {
        ArrayType::String
    } else if data.is_exact_instance_of::<PyBytes>() {
        ArrayType::Bytes
    } else if data.is_exact_instance_of::<PyDateTime>() {
        if !data.getattr("tzinfo")?.is_none() {
            return Err(ConstructionError::new_err(
                "aware datetimes require an explicit temporal kind",
            ));
        }
        ArrayType::ChronoDateTime
    } else if data.is_exact_instance_of::<PyDate>() {
        ArrayType::ChronoDate
    } else if data.is_exact_instance_of::<PyTime>() {
        ArrayType::ChronoTime
    } else if instance(data, "decimal", "Decimal")? {
        ArrayType::Decimal
    } else if instance(data, "uuid", "UUID")? {
        ArrayType::Uuid
    } else {
        return Err(ConstructionError::new_err(
            "ambiguous or unsupported value: supply an explicit kind",
        ));
    };
    Ok(kind)
}

fn instance(data: &Bound<'_, PyAny>, module: &str, name: &str) -> PyResult<bool> {
    Ok(data
        .get_type()
        .is(&data.py().import(module)?.getattr(name)?))
}

pub(super) fn require_sequence(data: &Bound<'_, PyAny>) -> PyResult<()> {
    if data.is_exact_instance_of::<PyList>() || data.is_exact_instance_of::<PyTuple>() {
        Ok(())
    } else {
        Err(ConstructionError::new_err("expected a list or tuple"))
    }
}

fn strict<'a, 'py, T: pyo3::type_object::PyTypeInfo>(
    data: &'a Bound<'py, PyAny>,
) -> PyResult<&'a Bound<'py, PyAny>> {
    if data.is_exact_instance_of::<T>() {
        Ok(data)
    } else {
        Err(ConstructionError::new_err(
            "value does not have the declared Python type",
        ))
    }
}

// [spec:pgorm:req:python.values]
pub(super) fn from_python(data: &Bound<'_, PyAny>, kind: &ArrayType) -> PyResult<Value> {
    let value = match kind {
        ArrayType::Bool => Value::Bool(Some(strict::<PyBool>(data)?.extract()?)),
        ArrayType::TinyInt => Value::TinyInt(Some(strict::<PyInt>(data)?.extract()?)),
        ArrayType::SmallInt => Value::SmallInt(Some(strict::<PyInt>(data)?.extract()?)),
        ArrayType::Int => Value::Int(Some(strict::<PyInt>(data)?.extract()?)),
        ArrayType::BigInt => Value::BigInt(Some(strict::<PyInt>(data)?.extract()?)),
        ArrayType::Unsigned => Value::Unsigned(Some(strict::<PyInt>(data)?.extract()?)),
        ArrayType::BigUnsigned => Value::BigUnsigned(Some(strict::<PyInt>(data)?.extract()?)),
        ArrayType::Float => Value::Float(Some(float32(data)?)),
        ArrayType::Double => Value::Double(Some(strict::<PyFloat>(data)?.extract()?)),
        ArrayType::String => Value::String(Some(Box::new(strict::<PyString>(data)?.extract()?))),
        ArrayType::Char => Value::Char(Some(strict::<PyString>(data)?.extract()?)),
        ArrayType::Bytes => Value::Bytes(Some(Box::new(strict::<PyBytes>(data)?.extract()?))),
        ArrayType::Json => Value::Json(Some(Box::new(json::from_python(data, 0)?))),
        ArrayType::Decimal => Value::Decimal(Some(Box::new(decimal(data)?))),
        ArrayType::Uuid => {
            if !instance(data, "uuid", "UUID")? {
                return Err(ConstructionError::new_err("expected uuid.UUID"));
            }
            Value::Uuid(Some(Box::new(data.extract::<Uuid>()?)))
        }
        ArrayType::IpNetwork => {
            let text = strict::<PyString>(data)?.extract::<&str>()?;
            Value::IpNetwork(Some(Box::new(
                text.parse::<IpNetwork>()
                    .map_err(|_| ConstructionError::new_err("invalid IP network"))?,
            )))
        }
        ArrayType::MacAddress => {
            let bytes: Vec<u8> = strict::<PyBytes>(data)?.extract()?;
            let bytes: [u8; 6] = bytes
                .try_into()
                .map_err(|_| ConstructionError::new_err("MAC address requires six bytes"))?;
            Value::MacAddress(Some(Box::new(MacAddress::new(bytes))))
        }
        ArrayType::Vector => {
            require_sequence(data)?;
            let values = data
                .try_iter()?
                .map(|item| float32(&item?))
                .collect::<PyResult<Vec<_>>>()?;
            Value::Vector(Some(Box::new(Vector::from(values))))
        }
        _ => temporal::from_python(data, kind)?,
    };
    Ok(value)
}

fn float32(data: &Bound<'_, PyAny>) -> PyResult<f32> {
    let number: f64 = strict::<PyFloat>(data)?.extract()?;
    let narrow = number as f32;
    // IEEE payloads, infinities, and signed zero must also survive the round trip.
    if (narrow as f64).to_bits() != number.to_bits() {
        return Err(ConstructionError::new_err(
            "float is not exactly representable as f32",
        ));
    }
    Ok(narrow)
}

fn decimal(data: &Bound<'_, PyAny>) -> PyResult<Decimal> {
    if !instance(data, "decimal", "Decimal")? {
        return Err(ConstructionError::new_err(
            "expected decimal.Decimal; floats are not accepted",
        ));
    }
    if !data.call_method0("is_finite")?.extract::<bool>()? {
        return Err(ConstructionError::new_err(
            "Rust Decimal does not support non-finite values",
        ));
    }
    let parts = data.call_method0("as_tuple")?;
    let exponent: i32 = parts.getattr("exponent")?.extract()?;
    if !(-28..=28).contains(&exponent) || parts.getattr("digits")?.len()? > 29 {
        return Err(ConstructionError::new_err(
            "Decimal exceeds Rust's exact 96-bit coefficient / scale 0–28",
        ));
    }
    let text: String = data.call_method1("__format__", ("f",))?.extract()?;
    let mut decimal = Decimal::from_str_exact(&text).map_err(|_| {
        ConstructionError::new_err("Decimal exceeds Rust's exact 96-bit coefficient / scale 0–28")
    })?;
    decimal.set_sign_negative(parts.getattr("sign")?.extract::<u8>()? != 0);
    Ok(decimal)
}

pub(super) fn to_python(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    macro_rules! native {
        ($value:expr) => {
            $value
                .as_ref()
                .map(|value| value.into_py_any(py))
                .transpose()
        };
    }
    macro_rules! boxed {
        ($value:expr) => {
            $value
                .as_deref()
                .map(|value| value.into_py_any(py))
                .transpose()
        };
    }
    let result = match value {
        Value::Bool(value) => native!(value),
        Value::TinyInt(value) => native!(value),
        Value::SmallInt(value) => native!(value),
        Value::Int(value) => native!(value),
        Value::BigInt(value) => native!(value),
        Value::Unsigned(value) => native!(value),
        Value::BigUnsigned(value) => native!(value),
        Value::Float(value) => value
            .map(|value| widen_float(value)?.into_py_any(py))
            .transpose(),
        Value::Double(value) => native!(value),
        Value::String(value) => boxed!(value),
        Value::Char(value) => native!(value),
        Value::Bytes(value) => Ok(value
            .as_ref()
            .map(|value| PyBytes::new(py, value).into_any().unbind())),
        Value::Json(value) => value
            .as_ref()
            .map(|value| json::to_python(py, value))
            .transpose(),
        Value::Decimal(value) => boxed!(value),
        Value::Uuid(value) => boxed!(value),
        Value::IpNetwork(value) => value
            .as_ref()
            .map(|value| {
                value
                    .to_string()
                    .into_pyobject(py)
                    .map(|v| v.into_any().unbind())
                    .map_err(Into::into)
            })
            .transpose(),
        Value::MacAddress(value) => Ok(value
            .as_ref()
            .map(|value| PyBytes::new(py, &value.bytes()).into_any().unbind())),
        Value::Vector(value) => value
            .as_ref()
            .map(|value| {
                let values = value
                    .to_vec()
                    .into_iter()
                    .map(widen_float)
                    .collect::<PyResult<Vec<_>>>()?;
                Ok(PyList::new(py, values)?.into_any().unbind())
            })
            .transpose(),
        Value::Array(_, value) => value
            .as_ref()
            .map(|values| {
                let items = values
                    .iter()
                    .map(|value| to_python(py, value))
                    .collect::<PyResult<Vec<_>>>()?;
                Ok(PyList::new(py, items)?.into_any().unbind())
            })
            .transpose(),
        _ => temporal::to_python(py, value),
    };
    result
        .map(|value| value.unwrap_or_else(|| py.None()))
        .map_err(|error: PyErr| DecodeError::new_err(error.to_string()))
}

fn widen_float(value: f32) -> PyResult<f64> {
    let wide = value as f64;
    if (wide as f32).to_bits() != value.to_bits() {
        return Err(DecodeError::new_err(
            "f32 NaN payload cannot be represented by a Python float",
        ));
    }
    Ok(wide)
}
