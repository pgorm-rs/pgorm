use jiff::{
    civil::{Date, DateTime, Time},
    tz::TimeZone,
};
use pgorm::pgorm_query::{ArrayType, Value};
use pyo3::{
    prelude::*,
    types::{PyDate, PyDateTime, PyDict, PyTime},
};

use crate::errors::{ConstructionError, DecodeError};

// [spec:pgorm:req:python.value-tags]
pub(super) fn from_python(data: &Bound<'_, PyAny>, kind: &ArrayType) -> PyResult<Value> {
    match kind {
        ArrayType::Date if data.is_exact_instance_of::<PyDate>() => {
            Ok(Value::Date(Some(Box::new(data.extract::<Date>()?))))
        }
        ArrayType::Time if data.is_exact_instance_of::<PyTime>() => {
            if !data.getattr("tzinfo")?.is_none() || data.getattr("fold")?.extract::<u8>()? != 0 {
                return Err(ConstructionError::new_err(
                    "time requires no timezone and fold=0",
                ));
            }
            Ok(Value::Time(Some(Box::new(data.extract::<Time>()?))))
        }
        ArrayType::DateTime if data.is_exact_instance_of::<PyDateTime>() => {
            if data.getattr("fold")?.extract::<u8>()? != 0 {
                return Err(ConstructionError::new_err("naive datetime requires fold=0"));
            }
            Ok(Value::DateTime(Some(Box::new(data.extract::<DateTime>()?))))
        }
        ArrayType::DateTimeWithTimeZone if data.is_exact_instance_of::<PyDateTime>() => aware(data),
        _ => Err(ConstructionError::new_err(
            "value does not have the declared temporal type",
        )),
    }
}

/// The offset is validated and discarded rather than stored: the one aware kind
/// is an instant, and an instant has nowhere to keep a timezone.
fn aware(data: &Bound<'_, PyAny>) -> PyResult<Value> {
    let offset = data.call_method0("utcoffset")?;
    if offset.is_none() {
        return Err(ConstructionError::new_err(
            "expected a timezone-aware datetime",
        ));
    }
    if offset.getattr("microseconds")?.extract::<u32>()? != 0 {
        return Err(ConstructionError::new_err(
            "subsecond timezone offsets cannot be represented by jiff",
        ));
    }
    if offset.getattr("days")?.extract::<i32>()? != 0
        || offset.getattr("seconds")?.extract::<i32>()? != 0
    {
        return Err(ConstructionError::new_err(
            "datetime_utc requires a zero UTC offset",
        ));
    }
    // A zero offset still resolves a fold, so read the wall clock the caller
    // chose rather than re-deriving one.
    let kwargs = PyDict::new(data.py());
    kwargs.set_item("tzinfo", data.py().None())?;
    kwargs.set_item("fold", 0)?;
    let naive: DateTime = data.call_method("replace", (), Some(&kwargs))?.extract()?;
    TimeZone::UTC
        .to_timestamp(naive)
        .map(|instant| Value::DateTimeWithTimeZone(Some(Box::new(instant))))
        .map_err(|_| ConstructionError::new_err("datetime is outside jiff's range"))
}

fn check_date(date: Date) -> PyResult<()> {
    if !(1..=9999).contains(&date.year()) {
        return Err(DecodeError::new_err(
            "date is outside Python's years 1–9999",
        ));
    }
    Ok(())
}

/// jiff cannot represent a leap second at all, so precision finer than a
/// microsecond is the only way a Rust time still outruns Python's.
fn check_time(time: Time) -> PyResult<()> {
    if !time.subsec_nanosecond().unsigned_abs().is_multiple_of(1000) {
        return Err(DecodeError::new_err(
            "Python time cannot represent sub-microsecond precision",
        ));
    }
    Ok(())
}

pub(super) fn to_python(py: Python<'_>, value: &Value) -> PyResult<Option<Py<PyAny>>> {
    match value {
        Value::Date(value) => value
            .as_ref()
            .map(|date| {
                check_date(**date)?;
                Ok((**date).into_pyobject(py)?.into_any().unbind())
            })
            .transpose(),
        Value::Time(value) => value
            .as_ref()
            .map(|time| {
                check_time(**time)?;
                Ok((**time).into_pyobject(py)?.into_any().unbind())
            })
            .transpose(),
        Value::DateTime(value) => value
            .as_ref()
            .map(|date| {
                check_date(date.date())?;
                check_time(date.time())?;
                Ok((**date).into_pyobject(py)?.into_any().unbind())
            })
            .transpose(),
        Value::DateTimeWithTimeZone(value) => value
            .as_ref()
            .map(|instant| {
                // Guard the civil fields Python is actually handed: an instant
                // reaches Python through its UTC zone.
                let civil = instant.to_zoned(TimeZone::UTC).datetime();
                check_date(civil.date())?;
                check_time(civil.time())?;
                Ok((**instant).into_pyobject(py)?.into_any().unbind())
            })
            .transpose(),
        _ => Err(DecodeError::new_err("not a temporal Rust value")),
    }
}
