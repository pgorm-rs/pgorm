//! `wire.validate`, reimplemented over `serde_json` values.
//!
//! The Python oracle runs this check on everything the subject reports, so
//! running it here first turns a malformed observation into a loud subject-side
//! failure instead of an oracle crash that looks like a finding.
//!
//! One gap is representational rather than a choice. Python's `json` decodes an
//! integer literal beyond `u64` as an `int` and rejects it; `serde_json`
//! without `arbitrary_precision` has already turned it into an `f64` by the
//! time validation sees it, so that case is caught earlier — at decode, by the
//! `ExactJson` codec — rather than here.

use pgorm::pgorm_query::IpNetwork;
use serde_json::{Map, Value as Json};
use uuid::Uuid;

use super::{SCALAR_NAMES, parse_date, parse_datetime_fixed, parse_datetime_utc};
use super::{parse_naive_datetime, parse_time};
use crate::FormatError;

const TEXT_BYTES: usize = 65_536;
const ITEMS: usize = 4_096;
const NAME_BYTES: usize = 63;
const JSON_DEPTH: usize = 64;

/// Integer kinds, as bit width and signedness.
const INTEGER_BITS: &[(&str, u32, bool)] = &[
    ("i8", 8, true),
    ("i16", 16, true),
    ("i32", 32, true),
    ("i64", 64, true),
    ("u32", 32, false),
    ("u64", 64, false),
];

/// Validate tagged data against the portable value format.
///
/// # Errors
///
/// Returns [`FormatError`] naming the first constraint the value breaks.
pub fn validate(value: &Json) -> Result<(), FormatError> {
    let object = fields(value, &["version", "type", "sql_null", "data"], &[])?;
    if object.get("version").and_then(Json::as_u64) != Some(1) {
        return Err(FormatError::new("unsupported value format version"));
    }
    let tag = object
        .get("type")
        .ok_or_else(|| FormatError::new("a type tag requires a kind"))?;
    let kind = type_tag(tag)?;
    let sql_null = object
        .get("sql_null")
        .and_then(Json::as_bool)
        .ok_or_else(|| FormatError::new("SQL NULL flag must be a boolean"))?;
    let data = object
        .get("data")
        .ok_or_else(|| FormatError::new("unexpected object fields; required: data"))?;
    if sql_null {
        if !data.is_null() {
            return Err(FormatError::new("SQL NULL cannot contain a data payload"));
        }
    } else if kind == "array" {
        let items = bounded_list(data, ITEMS)
            .ok_or_else(|| FormatError::new("array payload must be a bounded list"))?;
        let element = &tag["element"];
        for item in items {
            validate(item)?;
            if item.get("type") != Some(element) {
                return Err(FormatError::new(
                    "array element has a different type identity",
                ));
            }
        }
    } else {
        scalar(kind, data)?;
    }
    Ok(())
}

fn fields<'a>(
    value: &'a Json,
    required: &[&str],
    optional: &[&str],
) -> Result<&'a Map<String, Json>, FormatError> {
    let object = value.as_object().ok_or_else(|| unexpected(required))?;
    let complete = required.iter().all(|name| object.contains_key(*name));
    let bounded = object
        .keys()
        .all(|name| required.contains(&name.as_str()) || optional.contains(&name.as_str()));
    if complete && bounded {
        Ok(object)
    } else {
        Err(unexpected(required))
    }
}

fn unexpected(required: &[&str]) -> FormatError {
    let mut names = required.to_vec();
    names.sort_unstable();
    FormatError::new(format!(
        "unexpected object fields; required: {}",
        names.join(", ")
    ))
}

fn type_tag(tag: &Json) -> Result<&str, FormatError> {
    let kind = tag
        .get("kind")
        .and_then(Json::as_str)
        .ok_or_else(|| FormatError::new("a type tag requires a kind"))?;
    if SCALAR_NAMES.contains(&kind) {
        fields(tag, &["kind"], &[])?;
    } else if kind == "enum" {
        fields(tag, &["kind", "name", "schema"], &[])?;
        identifier(&tag["name"])?;
        if !tag["schema"].is_null() {
            identifier(&tag["schema"])?;
        }
    } else if kind == "array" {
        fields(tag, &["kind", "element"], &[])?;
        if type_tag(&tag["element"])? == "array" {
            return Err(FormatError::new("nested arrays are unsupported"));
        }
    } else {
        return Err(FormatError::new("unknown portable value kind"));
    }
    Ok(kind)
}

fn identifier(value: &Json) -> Result<(), FormatError> {
    let name = value.as_str().unwrap_or_default();
    if name.is_empty() || name.contains('\0') || name.len() > NAME_BYTES {
        return Err(FormatError::new(
            "identifiers must carry 1-63 UTF-8 bytes without NUL",
        ));
    }
    Ok(())
}

fn bounded_list(value: &Json, limit: usize) -> Option<&Vec<Json>> {
    value.as_array().filter(|items| items.len() <= limit)
}

fn scalar(kind: &str, data: &Json) -> Result<(), FormatError> {
    if let Some((_, bits, signed)) = INTEGER_BITS.iter().find(|(name, ..)| *name == kind) {
        return integer(data, *bits, *signed);
    }
    match kind {
        "f32" => bit_pattern(data, 8),
        "f64" => bit_pattern(data, 16),
        "bool" => data
            .is_boolean()
            .then_some(())
            .ok_or_else(|| FormatError::new("bool payload requires an exact boolean")),
        "text" | "char" | "enum" => text(data, kind == "char"),
        "bytes" => bytes(data, None),
        "mac_address" => bytes(data, Some(6)),
        "decimal" => decimal(data),
        "uuid" => exact(data, |text| {
            Uuid::parse_str(text).ok().map(|value| value.to_string())
        })
        .map_err(|()| FormatError::new("UUID payload must be canonical text")),
        "ipnetwork" => exact(data, |text| {
            text.parse::<IpNetwork>()
                .ok()
                .map(|value| value.to_string())
        })
        .map_err(|()| {
            FormatError::new("IP network payload must preserve canonical host/prefix text")
        }),
        "json" => json(data, 0),
        "vector" => vector(data),
        "date" | "time" => temporal(kind, data),
        _ if kind.starts_with("datetime") => temporal(kind, data),
        _ => Err(FormatError::new("unknown portable value kind")),
    }
}

fn integer(data: &Json, bits: u32, signed: bool) -> Result<(), FormatError> {
    let text = data
        .as_str()
        .filter(|text| canonical_integer(text))
        .ok_or_else(|| FormatError::new("integer payload must be canonical decimal text"))?;
    let number: i128 = text
        .parse()
        .map_err(|_| FormatError::new("integer payload must be canonical decimal text"))?;
    let low = if signed { -(1i128 << (bits - 1)) } else { 0 };
    let high = (1i128 << (bits - u32::from(signed))) - 1;
    if (low..=high).contains(&number) {
        Ok(())
    } else {
        Err(FormatError::new(
            "integer payload is outside its declared Rust variant",
        ))
    }
}

/// `re.fullmatch(r"0|-?[1-9][0-9]{0,19}", data)`.
fn canonical_integer(text: &str) -> bool {
    if text == "0" {
        return true;
    }
    let digits = text.strip_prefix('-').unwrap_or(text);
    let mut characters = digits.bytes();
    matches!(characters.next(), Some(b'1'..=b'9'))
        && digits.len() <= 20
        && characters.all(|byte| byte.is_ascii_digit())
}

fn bit_pattern(data: &Json, size: usize) -> Result<(), FormatError> {
    let valid = data.as_str().is_some_and(|text| {
        text.len() == size
            && text
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    });
    if valid {
        Ok(())
    } else {
        Err(FormatError::new(
            "float payload must be an exact lowercase IEEE bit pattern",
        ))
    }
}

fn text(data: &Json, single: bool) -> Result<(), FormatError> {
    let value = data
        .as_str()
        .filter(|value| value.len() <= TEXT_BYTES)
        .ok_or_else(|| FormatError::new("text payload must be a bounded Unicode string"))?;
    if single && value.chars().count() != 1 {
        return Err(FormatError::new(
            "character payload must have one Unicode scalar",
        ));
    }
    Ok(())
}

fn bytes(data: &Json, length: Option<usize>) -> Result<(), FormatError> {
    let items = bounded_list(data, TEXT_BYTES)
        .filter(|items| {
            items
                .iter()
                .all(|item| item.as_u64().is_some_and(|byte| byte <= 255))
        })
        .ok_or_else(|| FormatError::new("byte payload must be a bounded list of bytes"))?;
    if length.is_some_and(|length| items.len() != length) {
        return Err(FormatError::new("byte payload has the wrong length"));
    }
    Ok(())
}

fn decimal(data: &Json) -> Result<(), FormatError> {
    let malformed = || FormatError::new("decimal payload must preserve its coefficient and scale");
    let value = data.as_str().ok_or_else(malformed)?;
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = match unsigned.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (unsigned, ""),
    };
    let shaped = (whole == "0"
        || (matches!(whole.bytes().next(), Some(b'1'..=b'9'))
            && whole.bytes().all(|byte| byte.is_ascii_digit())))
        && (unsigned.find('.').is_none()
            || (!fraction.is_empty() && fraction.bytes().all(|byte| byte.is_ascii_digit())));
    if !shaped {
        return Err(malformed());
    }
    let coefficient: u128 = format!("{whole}{fraction}")
        .parse()
        .map_err(|_| FormatError::new("decimal payload exceeds the native coefficient or scale"))?;
    if fraction.len() > 28 || coefficient >= 1u128 << 96 {
        return Err(FormatError::new(
            "decimal payload exceeds the native coefficient or scale",
        ));
    }
    Ok(())
}

fn exact(data: &Json, canonicalize: impl Fn(&str) -> Option<String>) -> Result<(), ()> {
    let value = data.as_str().ok_or(())?;
    match canonicalize(value) {
        Some(canonical) if canonical == value => Ok(()),
        _ => Err(()),
    }
}

fn vector(data: &Json) -> Result<(), FormatError> {
    let items = bounded_list(data, ITEMS)
        .ok_or_else(|| FormatError::new("vector payload must be a bounded list"))?;
    items.iter().try_for_each(|item| bit_pattern(item, 8))
}

fn temporal(kind: &str, data: &Json) -> Result<(), FormatError> {
    let value = data
        .as_str()
        .ok_or_else(|| FormatError::new("temporal payload must be exact ISO text"))?;
    match kind {
        "date" => parse_date(value).map(drop),
        "time" => parse_time(value).map(drop),
        "datetime" => parse_naive_datetime(value).map(drop),
        "datetime_utc" => parse_datetime_utc(value).map(drop),
        _ => parse_datetime_fixed(value).map(drop),
    }
}

fn json(value: &Json, depth: usize) -> Result<(), FormatError> {
    if depth > JSON_DEPTH {
        return Err(FormatError::new("JSON payload exceeds its nesting budget"));
    }
    match value {
        Json::Null | Json::Bool(_) | Json::String(_) | Json::Number(_) => Ok(()),
        Json::Array(items) if items.len() <= ITEMS => {
            items.iter().try_for_each(|item| json(item, depth + 1))
        }
        Json::Object(entries) if entries.len() <= ITEMS => {
            entries.values().try_for_each(|item| json(item, depth + 1))
        }
        _ => Err(FormatError::new(
            "JSON payload cannot preserve the given value",
        )),
    }
}
