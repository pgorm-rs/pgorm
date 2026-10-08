use std::ops::Bound;

use pgorm::pgorm_query::{Range, RangeType, Value};
use pyo3::PyResult;
use serde_json::{Value as Json, json};

use super::{
    PyValue, ranges,
    types::{Tag, is_null},
};

// [spec:pgorm:req:python.value-tags]
pub(super) fn encode(value: &PyValue) -> PyResult<Json> {
    Ok(
        json!({"version": 1, "type": tag(&value.tag), "sql_null": is_null(&value.inner), "data": payload(value)?}),
    )
}

fn tag(value: &Tag) -> Json {
    match value {
        Tag::Scalar(_) => json!({"kind": value.name()}),
        Tag::Enum(name) => json!({"kind": "enum", "name": name.name, "schema": name.schema}),
        Tag::Created(kind) => json!({
            "kind": kind.kind_name(), "name": kind.name.name, "schema": kind.name.schema,
            "subtype": super::types::scalar_name(&kind.subtype),
        }),
        Tag::Array(element) => json!({"kind": "array", "element": tag(element)}),
    }
}

fn payload(value: &PyValue) -> PyResult<Json> {
    macro_rules! string {
        ($value:expr) => {
            json!($value.as_ref().map(|value| value.to_string()))
        };
    }
    Ok(match &value.inner {
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
        Value::Date(value) => string!(value),
        Value::Time(value) => string!(value),
        Value::DateTime(value) => string!(value),
        Value::DateTimeWithTimeZone(value) => string!(value),
        Value::IpNetwork(value) => string!(value),
        Value::MacAddress(value) => json!(value.as_ref().map(|value| value.bytes())),
        Value::Vector(value) => json!(value.as_ref().map(|value| {
            value
                .to_vec()
                .iter()
                .map(|value| format!("{:08x}", value.to_bits()))
                .collect::<Vec<_>>()
        })),
        Value::Array(_, values) => json!(
            values
                .as_ref()
                .map(|values| {
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
                        .collect::<PyResult<Vec<_>>>()
                })
                .transpose()?
        ),
        Value::Range(range, value) => match value {
            Some(value) => range_payload(*range, value)?,
            None => Json::Null,
        },
        Value::Multirange(range, value) => match value {
            Some(value) => Json::Array(
                value
                    .iter()
                    .map(|value| range_payload(*range, value))
                    .collect::<PyResult<_>>()?,
            ),
            None => Json::Null,
        },
    })
}

/// `{"empty": true}`, or each bound's own payload (`null` for no bound) and
/// the two brackets.
fn range_payload(range: RangeType, value: &Range<Value>) -> PyResult<Json> {
    let Range::Bounds { lower, upper } = value else {
        return Ok(json!({"empty": true}));
    };
    let bound = |bound: &Bound<Value>| -> PyResult<(Json, bool)> {
        Ok(match bound {
            Bound::Included(inner) | Bound::Excluded(inner) => (
                payload(&PyValue {
                    inner: inner.clone(),
                    tag: Tag::Scalar(ranges::element(range)),
                })?,
                matches!(bound, Bound::Included(_)),
            ),
            Bound::Unbounded => (Json::Null, false),
        })
    };
    let ((lower, open), (upper, close)) = (bound(lower)?, bound(upper)?);
    let bounds = format!(
        "{}{}",
        if open && !lower.is_null() { '[' } else { '(' },
        if close && !upper.is_null() { ']' } else { ')' }
    );
    Ok(json!({"lower": lower, "upper": upper, "bounds": bounds}))
}
