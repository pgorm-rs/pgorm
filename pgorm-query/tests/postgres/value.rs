use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use super::*;
use crate::oracle::assert_eq;

fn json_of(value: Value) -> serde_json::Value {
    sea_value_to_json_value(&value)
}

// [spec:pgorm:def:sql.value+1/test]    every variant wraps an Option; None keeps the type tag
#[test]
fn null_is_typed_none_not_shared_null() {
    let int_null: Value = Option::<i32>::None.into();
    let big_int_null: Value = Option::<i64>::None.into();

    assert_eq!(int_null, Value::Int(None));
    assert_eq!(big_int_null, Value::BigInt(None));
    // Both are SQL NULL, but they are not the same `Value` — the variant carries
    // the type even when the payload is absent.
    assert_ne!(int_null, big_int_null);
    assert_eq!(int_null.to_string(), "NULL");
    assert_eq!(big_int_null.to_string(), "NULL");
}

// [spec:pgorm:def:sql.value+1/test]    payloads larger than a pointer are boxed
#[test]
fn oversized_payloads_boxed_to_keep_enum_small() {
    // `String`, `Vec<u8>`, `serde_json::Value`, `DateTime` and friends are all
    // behind a `Box`, so `Value` stays a tag plus at most one pointer-ish payload.
    assert!(size_of::<Value>() <= 4 * size_of::<usize>());
    assert!(size_of::<Value>() < size_of::<Vec<u8>>() + size_of::<serde_json::Value>());
}

// [spec:pgorm:def:sql.value+1/test]    derived PartialEq, blanket Eq and Hash
#[test]
fn value_is_hashable_as_map_key() {
    let mut map: HashMap<Value, &str> = HashMap::new();
    map.insert(Value::Int(Some(1)), "one");
    map.insert(Value::String(Some(Box::new("k".to_owned()))), "kay");
    map.insert(Value::Double(Some(2.5)), "two-point-five");
    map.insert(
        Value::Vector(Some(Box::new(pgvector::Vector::from(vec![1.0, 2.0])))),
        "vector",
    );

    assert_eq!(map.get(&Value::Int(Some(1))), Some(&"one"));
    assert_eq!(
        map.get(&Value::String(Some(Box::new("k".to_owned())))),
        Some(&"kay")
    );
    assert_eq!(map.get(&Value::Double(Some(2.5))), Some(&"two-point-five"));
    assert_eq!(
        map.get(&Value::Vector(Some(Box::new(pgvector::Vector::from(
            vec![1.0, 2.0]
        ))))),
        Some(&"vector")
    );

    // Floats hash by their bits (so a NULL float hashes too), while equality keeps
    // IEEE semantics: NaN is not equal to itself.
    let nan = Value::Double(Some(f64::NAN));
    let also_nan = Value::Double(Some(f64::NAN));
    assert_ne!(nan, also_nan);
    let mut nan_map: HashMap<Value, &str> = HashMap::new();
    nan_map.insert(Value::Float(None), "null float");
    assert_eq!(nan_map.get(&Value::Float(None)), Some(&"null float"));
}

// [spec:pgorm:def:sql.value+1/test]    Display renders the Postgres SQL literal
#[test]
fn display_renders_a_postgres_literal() {
    assert_eq!(Value::Bool(Some(true)).to_string(), "TRUE");
    assert_eq!(Value::Bool(Some(false)).to_string(), "FALSE");
    assert_eq!(Value::Int(Some(-7)).to_string(), "-7");
    assert_eq!(
        Value::String(Some(Box::new("it".to_owned()))).to_string(),
        "'it'"
    );
    assert_eq!(
        Value::Bytes(Some(Box::new(vec![0xAB, 0x01]))).to_string(),
        r"'\xAB01'"
    );
    assert_eq!(
        Value::Array(
            ArrayType::Int,
            Some(Box::new(vec![Value::Int(Some(1)), Value::Int(Some(2))]))
        )
        .to_string(),
        "ARRAY [1,2]"
    );
}

// [spec:pgorm:def:sql.value+1/test]    the two surviving unsigned variants
#[test]
fn unsigned_variants_carry_oid_and_limit_counts() {
    let oid: Value = 42u32.into();
    let count: Value = 42u64.into();

    assert_eq!(oid, Value::Unsigned(Some(42)));
    assert_eq!(count, Value::BigUnsigned(Some(42)));

    // A `LIMIT`/`OFFSET` count reaches the builder as `BigUnsigned`.
    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .limit(10)
            .build(QueryBuilder),
        (
            r#"SELECT "id" FROM "glyph" LIMIT $1"#.to_owned(),
            Values(vec![Value::BigUnsigned(Some(10))])
        )
    );
}

// [spec:pgorm:def:sql.value.value-type+1/test]    try_from errors on a variant mismatch
#[test]
fn try_from_errors_on_variant_mismatch() {
    assert!(<i32 as ValueType>::try_from(Value::BigInt(Some(1))).is_err());
}

// [spec:pgorm:def:sql.value.value-type+1/test]    try_from errors on a nullability mismatch
#[test]
fn try_from_errors_on_a_null_payload() {
    assert!(<i32 as ValueType>::try_from(Value::Int(None)).is_err());
}

// [spec:pgorm:sem:sql.value.accessor-panics+1/test]    `as_ref_*` yields None for SQL NULL of the
// right variant
#[test]
fn as_ref_accessors_return_none_on_typed_null() {
    assert!(Value::Json(None).as_ref_json().is_none());
    assert!(Value::ChronoDate(None).as_ref_chrono_date().is_none());
    assert!(Value::Decimal(None).as_ref_decimal().is_none());
    assert!(Value::Uuid(None).as_ref_uuid().is_none());
    assert!(Value::Array(ArrayType::Int, None).as_ref_array().is_none());
    assert!(Value::IpNetwork(None).as_ref_ipnetwork().is_none());
    assert!(Value::MacAddress(None).as_ref_mac_address().is_none());

    let uuid = Uuid::nil();
    assert_eq!(Value::Uuid(Some(Box::new(uuid))).as_ref_uuid(), Some(&uuid));

    assert!(!Value::Int(Some(1)).is_json());
    assert!(Value::Json(None).is_json());
}

// [spec:pgorm:sem:sql.value.accessor-panics+1/test]    None is ambiguous: the wrong variant reads
// the same as a typed NULL, and only the `is_*` discriminator separates them
#[test]
fn as_ref_json_returns_none_on_another_variant() {
    assert!(Value::Int(Some(1)).as_ref_json().is_none());
    assert!(Value::Json(None).as_ref_json().is_none());
    assert!(!Value::Int(Some(1)).is_json());
    assert!(Value::Json(None).is_json());
}

// [spec:pgorm:sem:sql.value.accessor-panics+1/test]
#[test]
fn as_ref_uuid_returns_none_on_another_variant() {
    assert!(Value::Int(Some(1)).as_ref_uuid().is_none());
    assert!(!Value::Int(Some(1)).is_uuid());
}

// [spec:pgorm:sem:sql.value.accessor-panics+1/test]    chrono accessor stringifies the UTC-naive form
#[test]
fn as_naive_utc_in_string_uses_naive_form() {
    let zoned = DateTime::parse_from_rfc3339("2020-01-01T02:02:02+08:00").unwrap();
    assert_eq!(
        Value::ChronoDateTimeWithTimeZone(Some(Box::new(zoned))).chrono_as_naive_utc_in_string(),
        Some("2019-12-31 18:02:02".to_owned())
    );

    let date = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
    assert_eq!(
        Value::ChronoDate(Some(Box::new(date))).chrono_as_naive_utc_in_string(),
        Some("2020-01-01".to_owned())
    );
    assert_eq!(
        Value::ChronoDate(None).chrono_as_naive_utc_in_string(),
        None
    );
}

// [spec:pgorm:sem:sql.value.accessor-panics+1/test]
#[test]
fn chrono_as_naive_utc_none_on_other_variant() {
    assert!(
        Value::Int(Some(1))
            .chrono_as_naive_utc_in_string()
            .is_none()
    );
}

// [spec:pgorm:sem:sql.value.accessor-panics+1/test]    `as_ipaddr` returns the network address
#[test]
fn as_ipaddr_returns_the_network_address() {
    let net = IpNetwork::from_str("10.1.2.3/32").unwrap();
    assert_eq!(
        Value::IpNetwork(Some(Box::new(net))).as_ipaddr(),
        Some(net.network())
    );
}

// [spec:pgorm:sem:sql.value.accessor-panics+1/test]
#[test]
fn as_ipaddr_returns_none_on_another_variant() {
    assert!(Value::Int(Some(1)).as_ipaddr().is_none());
    assert!(Value::IpNetwork(None).as_ipaddr().is_none());
}

// [spec:pgorm:sem:sql.value.accessor-panics+1/test]    `decimal_to_f64` goes through the payload
#[test]
fn decimal_to_f64_converts_the_payload() {
    let decimal = Decimal::from_str("2.02").unwrap();
    assert_eq!(
        Value::Decimal(Some(Box::new(decimal))).decimal_to_f64(),
        Some(2.02)
    );
    assert_eq!(Value::Decimal(None).decimal_to_f64(), None);
}

// [spec:pgorm:sem:sql.value.to-json/test]    non-chrono NULLs map to JSON null
#[test]
fn to_json_maps_typed_nulls_to_json_null() {
    for value in [
        Value::Bool(None),
        Value::TinyInt(None),
        Value::SmallInt(None),
        Value::Int(None),
        Value::BigInt(None),
        Value::Unsigned(None),
        Value::BigUnsigned(None),
        Value::Float(None),
        Value::Double(None),
        Value::String(None),
        Value::Char(None),
        Value::Bytes(None),
        Value::Json(None),
        Value::Decimal(None),
        Value::Uuid(None),
        Value::Array(ArrayType::Int, None),
        Value::Vector(None),
        Value::IpNetwork(None),
        Value::MacAddress(None),
    ] {
        assert_eq!(json_of(value), serde_json::Value::Null);
    }
}

// [spec:pgorm:sem:sql.value.to-json/test]    scalars map to native JSON values
#[test]
fn to_json_maps_scalars_natively() {
    assert_eq!(json_of(Value::Bool(Some(true))), json!(true));
    assert_eq!(json_of(Value::TinyInt(Some(-1))), json!(-1));
    assert_eq!(json_of(Value::SmallInt(Some(2))), json!(2));
    assert_eq!(json_of(Value::Int(Some(3))), json!(3));
    assert_eq!(json_of(Value::BigInt(Some(4))), json!(4));
    assert_eq!(json_of(Value::Unsigned(Some(5))), json!(5));
    assert_eq!(json_of(Value::BigUnsigned(Some(6))), json!(6));
    assert_eq!(json_of(Value::Double(Some(2.5))), json!(2.5));
    assert_eq!(
        json_of(Value::String(Some(Box::new("hello".to_owned())))),
        json!("hello")
    );
    assert_eq!(json_of(Value::Char(Some('x'))), json!("x"));
    assert_eq!(
        json_of(Value::Json(Some(Box::new(json!({ "a": 1 }))))),
        json!({ "a": 1 })
    );
    assert_eq!(
        json_of(Value::Uuid(Some(Box::new(Uuid::nil())))),
        json!("00000000-0000-0000-0000-000000000000")
    );
    assert_eq!(
        json_of(Value::Decimal(Some(Box::new(
            Decimal::from_str("2.02").unwrap()
        )))),
        json!(2.02)
    );
    assert_eq!(
        json_of(Value::Bytes(Some(Box::new(b"bytes".to_vec())))),
        json!("bytes")
    );
}

// [spec:pgorm:def:sql.render.value-literals+1/test]    a char renders as its whole
// UTF-8 text, quoted and escaped exactly like a one-character string
#[test]
fn char_renders_whole_scalar_not_low_byte() {
    assert_eq!(Value::Char(Some('a')).to_string(), "'a'");
    assert_eq!(Value::Char(Some('é')).to_string(), "'é'");
    assert_eq!(Value::Char(Some('—')).to_string(), "'—'");
    assert_eq!(Value::Char(Some('\'')).to_string(), r"E'\''");
    assert_eq!(
        Value::Char(Some('é')).to_string(),
        Value::String(Some(Box::new("é".to_owned()))).to_string()
    );
}

// [spec:pgorm:def:sql.render.value-literals+1/test]    the char literals the renderer
// emits are ones the PostgreSQL grammar accepts
#[test]
fn char_literals_parse_as_postgres_literals() {
    for (character, expected) in [
        ('a', "SELECT 'a'"),
        ('é', "SELECT 'é'"),
        ('—', "SELECT '—'"),
        ('\'', r"SELECT E'\''"),
    ] {
        assert_eq!(
            Query::select()
                .expr(Value::Char(Some(character)))
                .to_string(QueryBuilder),
            expected
        );
    }
}

// [spec:pgorm:sem:sql.value.to-json/test]    a non-ASCII char keeps its scalar value
#[test]
fn to_json_keeps_non_ascii_chars_whole() {
    assert_eq!(json_of(Value::Char(Some('é'))), json!("é"));
    assert_eq!(json_of(Value::Char(Some('—'))), json!("—"));
}

// [spec:pgorm:sem:sql.value.to-json/test]    `Bytes` goes through `from_utf8(..).unwrap()`
#[test]
#[should_panic]
fn to_json_panics_on_non_utf8_bytes() {
    json_of(Value::Bytes(Some(Box::new(vec![0xFF, 0xFE]))));
}

// [spec:pgorm:sem:sql.value.to-json/test]    arrays recurse, vectors become number arrays
#[test]
fn to_json_maps_arrays_and_vectors() {
    assert_eq!(
        json_of(Value::Array(
            ArrayType::Int,
            Some(Box::new(vec![Value::Int(Some(1)), Value::Int(None)]))
        )),
        json!([1, null])
    );
    assert_eq!(
        json_of(Value::Vector(Some(Box::new(pgvector::Vector::from(vec![
            1.0, 2.5
        ]))))),
        json!([1.0, 2.5])
    );
}

// [spec:pgorm:sem:sql.value.to-json/test]    chrono values (and their NULLs) are stringified
// through `value_to_string`, quotes included
#[test]
fn to_json_stringifies_chrono_values_including_their_nulls() {
    let date = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
    assert_eq!(
        json_of(Value::ChronoDate(Some(Box::new(date)))),
        json!("'2020-01-01'")
    );

    let naive = date.and_hms_opt(2, 2, 2).unwrap();
    assert_eq!(
        json_of(Value::ChronoDateTime(Some(Box::new(naive)))),
        json!("'2020-01-01 02:02:02'")
    );
    assert_eq!(
        json_of(Value::ChronoTime(Some(Box::new(naive.time())))),
        json!("'02:02:02'")
    );
    assert_eq!(
        json_of(Value::ChronoDateTimeUtc(Some(Box::new(
            DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)
        )))),
        json!("'2020-01-01 02:02:02 +00:00'")
    );

    // A chrono NULL is not covered by the null arms, so it becomes the JSON
    // string "NULL" rather than JSON null.
    assert_eq!(json_of(Value::ChronoDate(None)), json!("NULL"));
    assert_eq!(json_of(Value::ChronoDateTime(None)), json!("NULL"));
    assert_eq!(json_of(Value::ChronoTime(None)), json!("NULL"));
    assert_eq!(json_of(Value::ChronoDateTimeUtc(None)), json!("NULL"));
    assert_eq!(json_of(Value::ChronoDateTimeLocal(None)), json!("NULL"));
    assert_eq!(
        json_of(Value::ChronoDateTimeWithTimeZone(None)),
        json!("NULL")
    );
}

// [spec:pgorm:sem:sql.value.to-json/test]    IpNetwork and MacAddress are stringified too
#[test]
fn to_json_stringifies_network_values() {
    let net = IpNetwork::from_str("10.1.2.3/32").unwrap();
    assert_eq!(
        json_of(Value::IpNetwork(Some(Box::new(net)))),
        serde_json::Value::String(format!("'{net}'"))
    );

    let mac = MacAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
    assert_eq!(
        json_of(Value::MacAddress(Some(Box::new(mac)))),
        serde_json::Value::String(format!("'{mac}'"))
    );
}
