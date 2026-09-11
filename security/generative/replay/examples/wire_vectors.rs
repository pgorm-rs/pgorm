//! The encoder's output and the validator's verdicts, as JSON lines, for
//! cross-checking against the Python source of truth.
//!
//! The unit tests pin what this crate encodes; this is how those pinned
//! documents are shown to be what `pgorm_campaign.wire` actually accepts. Run
//! it after touching the encoder, the validator, or `wire.py`:
//!
//! ```text
//! cargo run --manifest-path security/generative/replay/Cargo.toml \
//!     --target-dir target --example wire_vectors -q > vectors.jsonl
//! PYTHONPATH=security/generative/src python3 - <<'PY'
//! import json
//! from pgorm_campaign import wire
//! for line in open("vectors.jsonl", encoding="utf-8"):
//!     case = json.loads(line)
//!     try:
//!         wire.validate(case["document"])
//!         ok = True
//!     except Exception:
//!         ok = False
//!     assert ok == case["rust_ok"], case["label"]
//! PY
//! ```
//!
//! A dev tool, so a failed construction should abort loudly rather than be
//! threaded through a result: the panic is the signal.
#![allow(clippy::print_stdout, clippy::unwrap_used)]

use chrono::{FixedOffset, NaiveDate, NaiveTime};
use pgorm::pgorm_query::{ArrayType, MacAddress, Value, Vector};
use pgorm_generative_replay::wire::{Tagged, TypeName, validate};
use rust_decimal::Decimal;
use serde_json::{Value as Json, json};

fn emit(label: &str, document: &Json) {
    println!(
        "{}",
        json!({
            "label": label,
            "rust_ok": validate(document).is_ok(),
            "document": document,
        })
    );
}

fn main() {
    let naive = NaiveDate::from_ymd_opt(2024, 1, 2)
        .unwrap()
        .and_time(NaiveTime::from_hms_micro_opt(3, 4, 5, 123_456).unwrap());
    let offset = FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap();
    let fixed = naive.and_local_timezone(offset).single().unwrap();

    let encoded: Vec<(&str, Tagged)> = vec![
        ("null-i32", Tagged::from_value(Value::Int(None))),
        (
            "null-array",
            Tagged::from_value(Value::Array(ArrayType::String, None)),
        ),
        (
            "empty-array",
            Tagged::from_value(Value::Array(ArrayType::String, Some(Box::new(Vec::new())))),
        ),
        (
            "array-with-nulls",
            Tagged::from_value(Value::Array(
                ArrayType::String,
                Some(Box::new(vec![
                    Value::String(Some(Box::new("%_".to_owned()))),
                    Value::String(None),
                ])),
            )),
        ),
        ("f32-neg-zero", Tagged::from_value(Value::Float(Some(-0.0)))),
        ("f32-nan", Tagged::from_value(Value::Float(Some(f32::NAN)))),
        (
            "f32-signalling-nan",
            Tagged::from_value(Value::Float(Some(f32::from_bits(0x7f80_0001)))),
        ),
        (
            "f64-neg-zero",
            Tagged::from_value(Value::Double(Some(-0.0))),
        ),
        ("f64-nan", Tagged::from_value(Value::Double(Some(f64::NAN)))),
        (
            "f64-neg-infinity",
            Tagged::from_value(Value::Double(Some(f64::NEG_INFINITY))),
        ),
        ("i8-min", Tagged::from_value(Value::TinyInt(Some(-128)))),
        (
            "i16-min",
            Tagged::from_value(Value::SmallInt(Some(i16::MIN))),
        ),
        ("i32-zero", Tagged::from_value(Value::Int(Some(0)))),
        ("i64-min", Tagged::from_value(Value::BigInt(Some(i64::MIN)))),
        (
            "u32-max",
            Tagged::from_value(Value::Unsigned(Some(u32::MAX))),
        ),
        (
            "u64-max",
            Tagged::from_value(Value::BigUnsigned(Some(u64::MAX))),
        ),
        (
            "decimal-scale",
            Tagged::from_value(Value::Decimal(Some(Box::new(
                Decimal::from_str_exact("123.4500").unwrap(),
            )))),
        ),
        (
            "decimal-zero-scale",
            Tagged::from_value(Value::Decimal(Some(Box::new(
                Decimal::from_str_exact("0.0000").unwrap(),
            )))),
        ),
        (
            "decimal-max",
            Tagged::from_value(Value::Decimal(Some(Box::new(
                Decimal::from_str_exact("79228162514264337593543950335").unwrap(),
            )))),
        ),
        (
            "decimal-negative",
            Tagged::from_value(Value::Decimal(Some(Box::new(
                Decimal::from_str_exact("-0.0001").unwrap(),
            )))),
        ),
        (
            "bytes",
            Tagged::from_value(Value::Bytes(Some(Box::new(vec![0, 127, 255])))),
        ),
        (
            "bytes-empty",
            Tagged::from_value(Value::Bytes(Some(Box::new(Vec::new())))),
        ),
        (
            "mac",
            Tagged::from_value(Value::MacAddress(Some(Box::new(MacAddress::new([
                1, 2, 3, 253, 254, 255,
            ]))))),
        ),
        (
            "vector",
            Tagged::from_value(Value::Vector(Some(Box::new(Vector::from(vec![
                -0.0f32, 1.0f32,
            ]))))),
        ),
        (
            "text-hostile",
            Tagged::from_value(Value::String(Some(Box::new("O'Brien \" 雪\\".to_owned())))),
        ),
        ("char", Tagged::from_value(Value::Char(Some('雪')))),
        ("bool", Tagged::from_value(Value::Bool(Some(false)))),
        (
            "json-null-payload",
            Tagged::from_value(Value::Json(Some(Box::new(Json::Null)))),
        ),
        (
            "json-object",
            Tagged::from_value(Value::Json(Some(Box::new(
                json!({"owner": "O'Brien 雪", "null": Json::Null}),
            )))),
        ),
        ("json-sql-null", Tagged::from_value(Value::Json(None))),
        (
            "date",
            Tagged::from_value(Value::ChronoDate(Some(Box::new(naive.date())))),
        ),
        (
            "time",
            Tagged::from_value(Value::ChronoTime(Some(Box::new(naive.time())))),
        ),
        (
            "time-whole-second",
            Tagged::from_value(Value::ChronoTime(Some(Box::new(
                NaiveTime::from_hms_opt(3, 4, 5).unwrap(),
            )))),
        ),
        (
            "datetime",
            Tagged::from_value(Value::ChronoDateTime(Some(Box::new(naive)))),
        ),
        (
            "datetime-utc",
            Tagged::from_value(Value::ChronoDateTimeUtc(Some(Box::new(naive.and_utc())))),
        ),
        (
            "datetime-fixed",
            Tagged::from_value(Value::ChronoDateTimeWithTimeZone(Some(Box::new(fixed)))),
        ),
        (
            "uuid",
            Tagged::from_value(Value::Uuid(Some(Box::new(
                "00000000-0000-0000-0000-000000000001".parse().unwrap(),
            )))),
        ),
        (
            "ipnetwork",
            Tagged::from_value(Value::IpNetwork(Some(Box::new(
                "10.0.0.1/24".parse().unwrap(),
            )))),
        ),
        (
            "enum",
            Tagged::from_enum(
                Value::String(Some(Box::new("O'Brien 雪".to_owned()))),
                TypeName::new("State\" 雪").in_schema("fixture"),
                false,
            ),
        ),
        (
            "enum-unqualified",
            Tagged::from_enum(
                Value::String(Some(Box::new("calm".to_owned()))),
                TypeName::new("mood"),
                false,
            ),
        ),
        (
            "enum-array",
            Tagged::from_enum(
                Value::Array(
                    ArrayType::String,
                    Some(Box::new(vec![
                        Value::String(Some(Box::new("calm".to_owned()))),
                        Value::String(None),
                    ])),
                ),
                TypeName::new("State\" 雪").in_schema("fixture"),
                true,
            ),
        ),
    ];
    for (label, value) in &encoded {
        emit(label, &value.encode());
    }

    // Hand-written documents around the format's edges. Some are valid — the
    // point is that both sides return the same verdict, not that they refuse.
    let edges = [
        (
            "bad-version",
            json!({"version": 2, "type": {"kind": "i32"}, "sql_null": true, "data": Json::Null}),
        ),
        (
            "null-with-payload",
            json!({"version": 1, "type": {"kind": "i32"}, "sql_null": true, "data": "0"}),
        ),
        (
            "integer-as-number",
            json!({"version": 1, "type": {"kind": "i32"}, "sql_null": false, "data": 0}),
        ),
        (
            "i8-overflow",
            json!({"version": 1, "type": {"kind": "i8"}, "sql_null": false, "data": "128"}),
        ),
        (
            "negative-zero-int",
            json!({"version": 1, "type": {"kind": "i32"}, "sql_null": false, "data": "-0"}),
        ),
        (
            "uppercase-bits",
            json!({"version": 1, "type": {"kind": "f32"}, "sql_null": false, "data": "7FC00000"}),
        ),
        (
            "short-bits",
            json!({"version": 1, "type": {"kind": "f32"}, "sql_null": false, "data": "7fc0000"}),
        ),
        (
            "two-char",
            json!({"version": 1, "type": {"kind": "char"}, "sql_null": false, "data": "ab"}),
        ),
        (
            "byte-overflow",
            json!({"version": 1, "type": {"kind": "bytes"}, "sql_null": false, "data": [256]}),
        ),
        (
            "short-mac",
            json!({"version": 1, "type": {"kind": "mac_address"}, "sql_null": false, "data": [1, 2]}),
        ),
        (
            "leading-zero-decimal",
            json!({"version": 1, "type": {"kind": "decimal"}, "sql_null": false, "data": "01.5"}),
        ),
        (
            "trailing-dot-decimal",
            json!({"version": 1, "type": {"kind": "decimal"}, "sql_null": false, "data": "1."}),
        ),
        (
            "scale-29-decimal",
            json!({"version": 1, "type": {"kind": "decimal"}, "sql_null": false, "data": "0.000000000000000000000000000001"}),
        ),
        (
            "uppercase-uuid",
            json!({"version": 1, "type": {"kind": "uuid"}, "sql_null": false, "data": "00000000-0000-0000-0000-00000000000A"}),
        ),
        (
            "bare-ip",
            json!({"version": 1, "type": {"kind": "ipnetwork"}, "sql_null": false, "data": "10.0.0.1"}),
        ),
        (
            "loose-date",
            json!({"version": 1, "type": {"kind": "date"}, "sql_null": false, "data": "2024-1-2"}),
        ),
        (
            "iso-t-datetime",
            json!({"version": 1, "type": {"kind": "datetime"}, "sql_null": false, "data": "2024-01-02T03:04:05"}),
        ),
        (
            "nanosecond-time",
            json!({"version": 1, "type": {"kind": "time"}, "sql_null": false, "data": "03:04:05.123456789"}),
        ),
        (
            "offset-on-naive",
            json!({"version": 1, "type": {"kind": "datetime"}, "sql_null": false, "data": "2024-01-02 03:04:05+00:00"}),
        ),
        (
            "naive-on-fixed",
            json!({"version": 1, "type": {"kind": "datetime_fixed"}, "sql_null": false, "data": "2024-01-02 03:04:05"}),
        ),
        (
            "nonzero-utc",
            json!({"version": 1, "type": {"kind": "datetime_utc"}, "sql_null": false, "data": "2024-01-02 03:04:05+05:30"}),
        ),
        (
            "unknown-kind",
            json!({"version": 1, "type": {"kind": "unknown"}, "sql_null": true, "data": Json::Null}),
        ),
        (
            "nested-array",
            json!({"version": 1, "type": {"kind": "array", "element": {"kind": "array", "element": {"kind": "i32"}}}, "sql_null": true, "data": Json::Null}),
        ),
        (
            "mixed-array",
            json!({"version": 1, "type": {"kind": "array", "element": {"kind": "text"}}, "sql_null": false, "data": [{"version": 1, "type": {"kind": "i32"}, "sql_null": false, "data": "1"}]}),
        ),
        (
            "long-enum-name",
            json!({"version": 1, "type": {"kind": "enum", "name": "n".repeat(64), "schema": Json::Null}, "sql_null": true, "data": Json::Null}),
        ),
        (
            "extra-field",
            json!({"version": 1, "type": {"kind": "i32"}, "sql_null": true, "data": Json::Null, "extra": 1}),
        ),
        (
            "scalar-tag-extra",
            json!({"version": 1, "type": {"kind": "i32", "name": "x"}, "sql_null": true, "data": Json::Null}),
        ),
    ];
    for (label, document) in &edges {
        emit(label, document);
    }
}
