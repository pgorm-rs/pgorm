//! Parity between the JavaScript builders and pgorm-query's own: each family's
//! cases, built here directly with pgorm-query, must build exactly the SQL and
//! values `tests/parity/<family>.json` holds, which the JavaScript suite holds
//! its own builders to in both runtimes. A case missing on either side fails.

mod merge;
mod select;
mod writes;

use std::collections::BTreeMap;

use pgorm::pgorm_query::{Name, Value, Values};
use serde_json::Value as Json;

use crate::values::value_tag;

/// A name the binding would take as an identifier: 1–63 bytes without NUL.
pub(super) fn n(name: &str) -> Name {
    assert!(
        (1..=63).contains(&name.len()) && !name.contains('\0'),
        "{name:?} is no identifier the binding takes"
    );
    Name::runtime(name)
}

/// A value as the golden file writes it: its kind, and a text the JavaScript
/// suite writes the same way from the `Value` it inspects.
fn canonical(value: &Value) -> [String; 2] {
    [value_tag(value).name().to_owned(), text(value)]
}

fn text(value: &Value) -> String {
    match value {
        Value::Bool(Some(b)) => b.to_string(),
        Value::TinyInt(Some(v)) => v.to_string(),
        Value::SmallInt(Some(v)) => v.to_string(),
        Value::Int(Some(v)) => v.to_string(),
        Value::BigInt(Some(v)) => v.to_string(),
        Value::Unsigned(Some(v)) => v.to_string(),
        Value::BigUnsigned(Some(v)) => v.to_string(),
        Value::Float(Some(v)) => f64::from(*v).to_string(),
        Value::Double(Some(v)) => v.to_string(),
        Value::String(Some(v)) => v.to_string(),
        Value::Char(Some(v)) => v.to_string(),
        Value::Json(Some(v)) => v.to_string(),
        Value::Decimal(Some(v)) => v.to_string(),
        Value::Uuid(Some(v)) => v.to_string(),
        Value::Date(Some(v)) => v.to_string(),
        Value::Time(Some(v)) => v.to_string(),
        Value::DateTime(Some(v)) => v.to_string(),
        Value::DateTimeWithTimeZone(Some(v)) => v.to_string(),
        Value::Array(_, Some(items)) => {
            let items: Vec<String> = items.iter().map(text).collect();
            format!("[{}]", items.join(","))
        }
        other if crate::values::value_is_null(other) => "null".to_owned(),
        other => panic!("no canonical text for {other:?}"),
    }
}

fn golden(file: &str) -> BTreeMap<String, (String, Vec<[String; 2]>)> {
    let cases: BTreeMap<String, Json> =
        serde_json::from_str(file).expect("the golden file is JSON");
    cases
        .into_iter()
        .map(|(name, case)| {
            let sql = case["sql"].as_str().expect("a case has its SQL").to_owned();
            let values = case["values"]
                .as_array()
                .expect("a case has its values")
                .iter()
                .map(|pair| {
                    let pair = pair.as_array().expect("a value is [kind, text]");
                    [
                        pair[0].as_str().expect("a kind").to_owned(),
                        pair[1].as_str().expect("a text").to_owned(),
                    ]
                })
                .collect();
            (name, (sql, values))
        })
        .collect()
}

/// Hold `built`, a family's cases built with pgorm-query, to its golden file.
fn check(file: &str, built: Vec<(&str, (String, Values))>) {
    let golden = golden(file);
    let names: Vec<&str> = built.iter().map(|(name, _)| *name).collect();
    let expected: Vec<&str> = golden.keys().map(String::as_str).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted, expected,
        "the Rust cases and the golden file name the same cases"
    );
    for (name, (sql, values)) in built {
        let (golden_sql, golden_values) = &golden[name];
        assert_eq!(&sql, golden_sql, "{name}: SQL");
        let values: Vec<[String; 2]> = values.0.iter().map(canonical).collect();
        assert_eq!(&values, golden_values, "{name}: values");
    }
}

// [spec:pgorm:req:napi.select/test]
// [spec:pgorm:req:napi.expressions/test]
#[test]
fn select_family_matches_its_golden_file() {
    check(
        include_str!("../../../tests/parity/select.json"),
        select::cases(),
    );
}

// [spec:pgorm:req:napi.writes/test]
#[test]
fn writes_family_matches_its_golden_file() {
    check(
        include_str!("../../../tests/parity/writes.json"),
        writes::cases(),
    );
}

// [spec:pgorm:req:napi.merge/test]
#[test]
fn merge_family_matches_its_golden_file() {
    check(
        include_str!("../../../tests/parity/merge.json"),
        merge::cases(),
    );
}
