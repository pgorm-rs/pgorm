use pgorm::pgorm_query::Value;
use serde_json::{Value as Json, json};

use super::{
    PyValue,
    types::{Tag, is_null},
};

// [spec:pgorm:req:python.value-tags]
pub(super) fn encode(value: &PyValue) -> Json {
    json!({"version": 1, "type": tag(&value.tag), "sql_null": is_null(&value.inner), "data": payload(value)})
}

fn tag(value: &Tag) -> Json {
    match value {
        Tag::Scalar(_) => json!({"kind": value.name()}),
        Tag::Enum(name) => json!({"kind": "enum", "name": name.name, "schema": name.schema}),
        Tag::Array(element) => json!({"kind": "array", "element": tag(element)}),
    }
}

fn payload(value: &PyValue) -> Json {
    macro_rules! string {
        ($value:expr) => {
            json!($value.as_ref().map(|value| value.to_string()))
        };
    }
    match &value.inner {
        Value::Bool(value) => json!(value),
        Value::TinyInt(value) => string!(value),
        Value::SmallInt(value) => string!(value),
        Value::Int(value) => string!(value),
        Value::BigInt(value) => string!(value),
        Value::Unsigned(value) => string!(value),
        Value::BigUnsigned(value) => string!(value),
        Value::Float(value) => json!(value.map(|value| format!("{:08x}", value.to_bits()))),
        Value::Double(value) => json!(value.map(|value| format!("{:016x}", value.to_bits()))),
        Value::String(value) => json!(value),
        Value::Char(value) => json!(value),
        Value::Bytes(value) => json!(value),
        Value::Json(value) => json!(value),
        Value::Decimal(value) => string!(value),
        Value::Uuid(value) => string!(value),
        Value::ChronoDate(value) => string!(value),
        Value::ChronoTime(value) => string!(value),
        Value::ChronoDateTime(value) => string!(value),
        Value::ChronoDateTimeUtc(value) => string!(value),
        Value::ChronoDateTimeLocal(value) => string!(value),
        Value::ChronoDateTimeWithTimeZone(value) => string!(value),
        Value::IpNetwork(value) => string!(value),
        Value::MacAddress(value) => json!(value.as_ref().map(|value| value.bytes())),
        Value::Vector(value) => json!(value.as_ref().map(|value| {
            value
                .to_vec()
                .iter()
                .map(|value| format!("{:08x}", value.to_bits()))
                .collect::<Vec<_>>()
        })),
        Value::Array(_, values) => json!(values.as_ref().map(|values| {
            values
                .iter()
                .map(|inner| {
                    let tag = match &value.tag {
                        Tag::Array(element) => (**element).clone(),
                        _ => super::types::rust_tag(inner),
                    };
                    encode(&PyValue {
                        inner: inner.clone(),
                        tag,
                    })
                })
                .collect::<Vec<_>>()
        })),
    }
}
