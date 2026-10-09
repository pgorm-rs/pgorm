//! JSON between serde_json, which pgorm's `Value::Json` holds, and JavaScript.
//!
//! A JSON number reaches JavaScript exactly or not at all. serde_json reads an
//! integer that fits 64 bits as one and anything else as an `f64`; the
//! document's own spelling is compared with what was read, so a number that
//! changed in the reading is refused rather than delivered rounded. The text
//! then crosses as text and `lib/values.js` parses it, turning an integer
//! literal past JavaScript's safe range into a `bigint`.

use std::{collections::BTreeMap, error::Error};

use neon::{prelude::*, types::JsBigInt};
use serde_json::{Map, Number, Value, value::RawValue};
use tokio_postgres::types::{FromSql, Type};

use super::read::{refuse, string};
use crate::codec::Codec;

type CodecError = Box<dyn Error + Send + Sync>;

/// How deep a document may nest, read or written.
const MAX_DEPTH: usize = 64;

/// The largest integer a JavaScript number holds exactly, 2^53 - 1.
pub(crate) const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// A `json` or `jsonb` column whose every number serde_json read exactly.
// [spec:pgorm:req:napi.values]
#[derive(Debug)]
pub(crate) struct ExactJson(pub(crate) Value);

impl<'a> FromSql<'a> for ExactJson {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, CodecError> {
        let value = Value::from_sql(ty, raw)?;
        let text = if *ty == Type::JSONB {
            raw.get(1..).ok_or("missing JSONB version")?
        } else {
            raw
        };
        let original: &RawValue = serde_json::from_slice(text)?;
        validate(original, &value, 0)?;
        Ok(Self(value))
    }

    fn accepts(ty: &Type) -> bool {
        <Value as FromSql>::accepts(ty)
    }
}

fn validate(raw: &RawValue, decoded: &Value, depth: usize) -> Result<(), CodecError> {
    if depth > MAX_DEPTH {
        return Err("JSON nests deeper than 64 levels".into());
    }
    match decoded {
        Value::Number(number) => {
            let original = raw.get();
            if !original.contains(['.', 'e', 'E']) && !number.is_i64() && !number.is_u64() {
                return Err(format!("JSON integer {original} exceeds 64 bits").into());
            }
            if canonical(original)? != canonical(&number.to_string())? {
                return Err(format!("JSON number {original} has no exact binary form").into());
            }
        }
        Value::Array(values) => {
            let originals: Vec<&RawValue> = serde_json::from_str(raw.get())?;
            if originals.len() != values.len() {
                return Err("JSON array shape changed".into());
            }
            for (original, value) in originals.into_iter().zip(values) {
                validate(original, value, depth + 1)?;
            }
        }
        Value::Object(values) => {
            let originals: BTreeMap<String, &RawValue> = serde_json::from_str(raw.get())?;
            for (name, value) in values {
                validate(
                    originals.get(name).ok_or("JSON object shape changed")?,
                    value,
                    depth + 1,
                )?;
            }
        }
        _ => (),
    }
    Ok(())
}

/// A decimal JSON spelling as sign, significant digits and exponent, compared
/// without converting either side through a float.
fn canonical(text: &str) -> Result<(bool, String, i64), CodecError> {
    let negative = text.starts_with('-');
    let text = text.strip_prefix('-').unwrap_or(text);
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i64>()?),
        None => (text, 0),
    };
    let fraction = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let significant = digits.trim_start_matches('0');
    if significant.is_empty() {
        return Ok((negative, "0".to_owned(), 0));
    }
    let trimmed = significant.trim_end_matches('0');
    let zeros = significant.len() - trimmed.len();
    let exponent = exponent
        .checked_sub(i64::try_from(fraction)?)
        .and_then(|exponent| exponent.checked_add(i64::try_from(zeros).ok()?))
        .ok_or("JSON exponent exceeds the supported range")?;
    Ok((negative, trimmed.to_owned(), exponent))
}

/// A JavaScript value as JSON: `null`, a boolean, a finite number, a bigint
/// within 64 bits, a string, an array, or an object of the module's plain
/// kind. Anything JSON cannot hold is refused, never dropped or turned into a
/// string as `JSON.stringify` would.
// [spec:pgorm:req:napi.values]
pub(crate) fn read<'cx>(
    cx: &mut Cx<'cx>,
    codec: Codec<'cx>,
    data: Handle<'cx, JsValue>,
    depth: usize,
) -> NeonResult<Value> {
    if depth > MAX_DEPTH {
        return refuse(cx, "JSON nests deeper than 64 levels, or holds a cycle");
    }
    if data.is_a::<JsNull, _>(cx) {
        return Ok(Value::Null);
    }
    if let Ok(flag) = data.downcast::<JsBoolean, _>(cx) {
        return Ok(Value::Bool(flag.value(cx)));
    }
    if let Ok(number) = data.downcast::<JsNumber, _>(cx) {
        let number = number.value(cx);
        if number.fract() == 0.0 && number.abs() <= MAX_SAFE_INTEGER {
            // An integral number keeps an integer's spelling, `1` not `1.0`;
            // negative zero is no integer and stays a float.
            if number != 0.0 || number.is_sign_positive() {
                #[allow(clippy::cast_possible_truncation)]
                return Ok(Value::Number((number as i64).into()));
            }
        }
        return match Number::from_f64(number) {
            Some(number) => Ok(Value::Number(number)),
            None => refuse(cx, "JSON has no NaN or infinite number"),
        };
    }
    if let Ok(big) = data.downcast::<JsBigInt, _>(cx) {
        if let Ok(value) = big.to_i64(cx) {
            return Ok(Value::Number(value.into()));
        }
        if let Ok(value) = big.to_u64(cx) {
            return Ok(Value::Number(value.into()));
        }
        return refuse(cx, "a JSON integer must fit 64 bits");
    }
    if data.is_a::<JsString, _>(cx) {
        return Ok(Value::String(string(cx, data)?));
    }
    if let Ok(array) = data.downcast::<JsArray, _>(cx) {
        let items = array.to_vec(cx)?;
        let mut values = Vec::with_capacity(items.len());
        for item in items {
            values.push(read(cx, codec, item, depth + 1)?);
        }
        return Ok(Value::Array(values));
    }
    if data.is_a::<JsObject, _>(cx) && !data.is_a::<JsFunction, _>(cx) {
        let described = codec.describe(cx, data)?;
        let Some(description) = described.filter(|description| description.tag == "json") else {
            return refuse(
                cx,
                "JSON holds only null, booleans, numbers, bigints, strings, arrays and plain \
                 objects; encode any other value as one of them first",
            );
        };
        let entries = description
            .field(cx, 0)
            .downcast_or_throw::<JsArray, _>(cx)?
            .to_vec(cx)?;
        let mut object = Map::new();
        for entry in entries {
            let entry = entry.downcast_or_throw::<JsArray, _>(cx)?;
            let key: Handle<JsValue> = entry.get(cx, 0)?;
            let key = string(cx, key)?;
            let value: Handle<JsValue> = entry.get(cx, 1)?;
            if value.is_a::<JsUndefined, _>(cx) {
                return refuse(
                    cx,
                    format!("JSON has no undefined: property {key:?} must be null or be left out"),
                );
            }
            object.insert(key, read(cx, codec, value, depth + 1)?);
        }
        return Ok(Value::Object(object));
    }
    refuse(
        cx,
        "JSON holds only null, booleans, numbers, bigints, strings, arrays and plain objects",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // [spec:pgorm:req:napi.values/test]
    #[test]
    fn json_numbers_are_read_exactly_or_refused() -> Result<(), CodecError> {
        for text in [
            "0.1",
            "-0.0",
            "1e20",
            "[1.2300,18446744073709551615]",
            "{\"a\": 1.2345e-8}",
        ] {
            assert_eq!(
                ExactJson::from_sql(&Type::JSON, text.as_bytes())?.0,
                Value::from_sql(&Type::JSON, text.as_bytes())?
            );
        }
        for text in [
            "18446744073709551616",
            "100000000000000000000",
            "[0.10000000000000000001]",
            "{\"a\":1e-400}",
        ] {
            assert!(
                ExactJson::from_sql(&Type::JSON, text.as_bytes()).is_err(),
                "{text}"
            );
        }
        Ok(())
    }
}
