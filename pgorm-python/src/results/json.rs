//! Validate numeric fidelity around serde_json's PostgreSQL FromSql decoder.

use std::{collections::BTreeMap, error::Error};

use serde_json::{Value, value::RawValue};
use tokio_postgres::types::{FromSql, Type};

type CodecError = Box<dyn Error + Send + Sync>;

#[derive(Debug)]
pub(super) struct ExactJson(pub Value);

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
    if depth > 64 {
        return Err("JSON exceeds 64 levels".into());
    }
    match decoded {
        Value::Number(number) => {
            let original = raw.get();
            if !original.contains(['.', 'e', 'E']) && !number.is_i64() && !number.is_u64() {
                return Err("JSON integer exceeds Rust's i64/u64 range".into());
            }
            if canonical(original)? != canonical(&number.to_string())? {
                return Err("JSON number cannot survive Rust serde_json decoding exactly".into());
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

/// Compare decimal JSON spellings without converting either through float.
/// Parsing and JSON structure remain owned by serde_json.
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
    let mut digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let significant = digits.trim_start_matches('0');
    if significant.is_empty() {
        return Ok((negative, "0".to_owned(), 0));
    }
    digits = significant.to_owned();
    let zeros = digits.len() - digits.trim_end_matches('0').len();
    digits.truncate(digits.len() - zeros);
    let exponent = exponent
        .checked_sub(i64::try_from(fraction)?)
        .and_then(|e| e.checked_add(zeros as i64))
        .ok_or("JSON exponent exceeds supported range")?;
    Ok((negative, digits, exponent))
}

#[cfg(test)]
mod tests {
    use super::*;

    // [spec:pgorm:req:python.results/test]
    #[test]
    fn json_numbers_reject_rounding_and_integer_loss() -> Result<(), CodecError> {
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
