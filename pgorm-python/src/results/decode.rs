use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use pgorm::pgorm_query::{ArrayType, Value, Vector};
use pyo3::prelude::*;
use tokio_postgres::{
    Row,
    types::{FromSql, Kind, Type},
};
use uuid::Uuid;

use super::codecs::{CheckedArray, EnumLabel, ExactDecimal, ExactTime, Inet, Mac};
use super::json::ExactJson;
use crate::{
    errors::DecodeError,
    values::{PyTypeName, PyValue},
};

fn read<T: for<'a> FromSql<'a>>(row: &Row, index: usize) -> PyResult<Option<T>> {
    row.try_get(index).map_err(|_| DecodeError::new_err(format!(
        "column {index} cannot be decoded without loss; its PostgreSQL type or value is unsupported"
    )))
}

fn typed<T: for<'a> FromSql<'a>>(
    row: &Row,
    index: usize,
    array: bool,
    kind: ArrayType,
    convert: impl Fn(Option<T>) -> Value,
) -> PyResult<Value> {
    if array {
        let values = read::<CheckedArray<T>>(row, index)?
            .map(|values| Box::new(values.0.into_iter().map(convert).collect()));
        Ok(Value::Array(kind, values))
    } else {
        read::<T>(row, index).map(convert)
    }
}

// [spec:pgorm:req:python.results]
// [spec:pgorm:req:python.value-tags]
pub(super) fn value(row: &Row, index: usize) -> PyResult<PyValue> {
    let declared = row.columns()[index].type_();
    let (ty, array) = match declared.kind() {
        Kind::Array(member) => (member, true),
        _ => (declared, false),
    };
    macro_rules! scalar {
        ($rust:ty, $variant:ident) => {
            typed::<$rust>(row, index, array, ArrayType::$variant, Value::$variant)
        };
    }
    macro_rules! boxed {
        ($rust:ty, $variant:ident) => {
            typed::<$rust>(row, index, array, ArrayType::$variant, |v| {
                Value::$variant(v.map(Box::new))
            })
        };
    }
    if matches!(ty.kind(), Kind::Enum(_)) {
        let inner = typed::<EnumLabel>(row, index, array, ArrayType::String, |v| {
            Value::String(v.map(|v| Box::new(v.0)))
        })?;
        return Ok(PyValue::from_enum(
            inner,
            PyTypeName {
                name: ty.name().to_owned(),
                schema: Some(ty.schema().to_owned()),
            },
            array,
        ));
    }
    let inner = match *ty {
        Type::BOOL => scalar!(bool, Bool),
        Type::CHAR => scalar!(i8, TinyInt),
        Type::INT2 => scalar!(i16, SmallInt),
        Type::INT4 => scalar!(i32, Int),
        Type::INT8 => scalar!(i64, BigInt),
        Type::OID => scalar!(u32, Unsigned),
        Type::FLOAT4 => scalar!(f32, Float),
        Type::FLOAT8 => scalar!(f64, Double),
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME => boxed!(String, String),
        Type::BYTEA => boxed!(Vec<u8>, Bytes),
        Type::JSON | Type::JSONB => typed::<ExactJson>(row, index, array, ArrayType::Json, |v| {
            Value::Json(v.map(|v| Box::new(v.0)))
        }),
        Type::UUID => boxed!(Uuid, Uuid),
        Type::DATE => boxed!(NaiveDate, ChronoDate),
        Type::TIME => typed::<ExactTime>(row, index, array, ArrayType::ChronoTime, |v| {
            Value::ChronoTime(v.map(|v| Box::new(v.0)))
        }),
        Type::TIMESTAMP => boxed!(NaiveDateTime, ChronoDateTime),
        Type::TIMESTAMPTZ => boxed!(DateTime<Utc>, ChronoDateTimeUtc),
        Type::NUMERIC => typed::<ExactDecimal>(row, index, array, ArrayType::Decimal, |v| {
            Value::Decimal(v.map(|v| Box::new(v.0)))
        }),
        Type::INET | Type::CIDR => typed::<Inet>(row, index, array, ArrayType::IpNetwork, |v| {
            Value::IpNetwork(v.map(|v| Box::new(v.0)))
        }),
        Type::MACADDR => typed::<Mac>(row, index, array, ArrayType::MacAddress, |v| {
            Value::MacAddress(v.map(|v| Box::new(v.0)))
        }),
        _ if ty.name() == "vector" && matches!(ty.kind(), Kind::Simple) => boxed!(Vector, Vector),
        _ => {
            return Err(DecodeError::new_err(format!(
                "column {index} has an unsupported PostgreSQL type"
            )));
        }
    }?;
    Ok(PyValue::from_rust(inner))
}
