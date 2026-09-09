use chrono::{
    DateTime, Datelike, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone,
    Timelike, Utc,
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
        ArrayType::ChronoDate if data.is_exact_instance_of::<PyDate>() => Ok(Value::ChronoDate(
            Some(Box::new(data.extract::<NaiveDate>()?)),
        )),
        ArrayType::ChronoTime if data.is_exact_instance_of::<PyTime>() => {
            if !data.getattr("tzinfo")?.is_none() || data.getattr("fold")?.extract::<u8>()? != 0 {
                return Err(ConstructionError::new_err(
                    "time requires no timezone and fold=0",
                ));
            }
            Ok(Value::ChronoTime(Some(Box::new(
                data.extract::<NaiveTime>()?,
            ))))
        }
        ArrayType::ChronoDateTime if data.is_exact_instance_of::<PyDateTime>() => {
            if data.getattr("fold")?.extract::<u8>()? != 0 {
                return Err(ConstructionError::new_err("naive datetime requires fold=0"));
            }
            Ok(Value::ChronoDateTime(Some(Box::new(
                data.extract::<NaiveDateTime>()?,
            ))))
        }
        ArrayType::ChronoDateTimeUtc
        | ArrayType::ChronoDateTimeLocal
        | ArrayType::ChronoDateTimeWithTimeZone
            if data.is_exact_instance_of::<PyDateTime>() =>
        {
            aware(data, kind)
        }
        _ => Err(ConstructionError::new_err(
            "value does not have the declared temporal type",
        )),
    }
}

fn aware(data: &Bound<'_, PyAny>, kind: &ArrayType) -> PyResult<Value> {
    let offset = data.call_method0("utcoffset")?;
    if offset.is_none() {
        return Err(ConstructionError::new_err(
            "expected a timezone-aware datetime",
        ));
    }
    if offset.getattr("microseconds")?.extract::<u32>()? != 0 {
        return Err(ConstructionError::new_err(
            "subsecond timezone offsets cannot be represented by chrono",
        ));
    }
    let seconds = offset
        .getattr("days")?
        .extract::<i32>()?
        .checked_mul(86400)
        .and_then(|days| {
            offset
                .getattr("seconds")
                .ok()?
                .extract::<i32>()
                .ok()?
                .checked_add(days)
        })
        .and_then(FixedOffset::east_opt)
        .ok_or_else(|| ConstructionError::new_err("invalid timezone offset"))?;
    let kwargs = PyDict::new(data.py());
    kwargs.set_item("tzinfo", data.py().None())?;
    kwargs.set_item("fold", 0)?;
    let naive: NaiveDateTime = data.call_method("replace", (), Some(&kwargs))?.extract()?;
    let date = seconds
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| ConstructionError::new_err("datetime is outside chrono's range"))?;
    match kind {
        ArrayType::ChronoDateTimeUtc => {
            if seconds.local_minus_utc() != 0 {
                return Err(ConstructionError::new_err(
                    "datetime_utc requires a zero UTC offset",
                ));
            }
            Ok(Value::ChronoDateTimeUtc(Some(Box::new(
                date.with_timezone(&Utc),
            ))))
        }
        ArrayType::ChronoDateTimeLocal => {
            let local = date.with_timezone(&Local);
            if local.naive_local() != naive || local.offset().fix() != seconds {
                return Err(ConstructionError::new_err(
                    "datetime_local must match the machine's local timezone at that instant",
                ));
            }
            Ok(Value::ChronoDateTimeLocal(Some(Box::new(local))))
        }
        _ => Ok(Value::ChronoDateTimeWithTimeZone(Some(Box::new(date)))),
    }
}

fn check_date(date: NaiveDate) -> PyResult<()> {
    if !(1..=9999).contains(&date.year()) {
        return Err(DecodeError::new_err(
            "date is outside Python's years 1–9999",
        ));
    }
    Ok(())
}

fn check_time(time: NaiveTime) -> PyResult<()> {
    if time.nanosecond() >= 1_000_000_000 || !time.nanosecond().is_multiple_of(1000) {
        return Err(DecodeError::new_err(
            "Python time cannot represent leap seconds or sub-microsecond precision",
        ));
    }
    Ok(())
}

fn date_time<Tz: TimeZone>(py: Python<'_>, date: &DateTime<Tz>) -> PyResult<Py<PyAny>> {
    check_date(date.date_naive())?;
    check_time(date.time())?;
    Ok(date.fixed_offset().into_pyobject(py)?.into_any().unbind())
}

pub(super) fn to_python(py: Python<'_>, value: &Value) -> PyResult<Option<Py<PyAny>>> {
    match value {
        Value::ChronoDate(value) => value
            .as_ref()
            .map(|date| {
                check_date(**date)?;
                Ok((**date).into_pyobject(py)?.into_any().unbind())
            })
            .transpose(),
        Value::ChronoTime(value) => value
            .as_ref()
            .map(|time| {
                check_time(**time)?;
                Ok((**time).into_pyobject(py)?.into_any().unbind())
            })
            .transpose(),
        Value::ChronoDateTime(value) => value
            .as_ref()
            .map(|date| {
                check_date(date.date())?;
                check_time(date.time())?;
                Ok((**date).into_pyobject(py)?.into_any().unbind())
            })
            .transpose(),
        Value::ChronoDateTimeUtc(value) => value
            .as_ref()
            .map(|date| date_time(py, date.as_ref()))
            .transpose(),
        Value::ChronoDateTimeLocal(value) => value
            .as_ref()
            .map(|date| date_time(py, date.as_ref()))
            .transpose(),
        Value::ChronoDateTimeWithTimeZone(value) => value
            .as_ref()
            .map(|date| date_time(py, date.as_ref()))
            .transpose(),
        _ => Err(DecodeError::new_err("not a temporal Rust value")),
    }
}
