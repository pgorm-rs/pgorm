//! Encoder tests, written against the payloads `pgorm_campaign.wire` accepts.
//!
//! Every expected document here has been checked against the Python source of
//! truth by feeding it to `wire.validate`; the assertions below pin the exact
//! text so a drift in chrono's `Display`, in `Decimal`'s scale handling or in
//! the float bit formatting fails here rather than in the oracle.

use std::error::Error;

use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use pgorm::pgorm_query::{ArrayType, MacAddress, Value, Vector};
use rust_decimal::Decimal;
use serde_json::{Value as Json, json};

use crate::{
    ObservedError, Report, entities, observe,
    wire::{Tag, Tagged, TypeName, parse_datetime_fixed, parse_datetime_utc, parse_naive_datetime},
    wire::{parse_date, parse_time, temporal_text, validate},
};

type TestResult = Result<(), Box<dyn Error + Send + Sync>>;

fn encoded(value: Value) -> Result<Json, Box<dyn Error + Send + Sync>> {
    Ok(Tagged::from_value(value).encode_checked()?)
}

fn text(value: &str) -> Value {
    Value::String(Some(Box::new(value.to_owned())))
}

fn moment() -> Result<NaiveDateTime, Box<dyn Error + Send + Sync>> {
    let date = NaiveDate::from_ymd_opt(2024, 1, 2).ok_or("unrepresentable date")?;
    let time = NaiveTime::from_hms_micro_opt(3, 4, 5, 123_456).ok_or("unrepresentable time")?;
    Ok(date.and_time(time))
}

#[test]
fn typed_null_keeps_its_kind_without_payload() -> TestResult {
    assert_eq!(
        encoded(Value::Int(None))?,
        json!({"version": 1, "type": {"kind": "i32"}, "sql_null": true, "data": Json::Null})
    );
    assert_eq!(
        encoded(Value::Array(ArrayType::String, None))?,
        json!({
            "version": 1,
            "type": {"kind": "array", "element": {"kind": "text"}},
            "sql_null": true,
            "data": Json::Null,
        })
    );
    Ok(())
}

#[test]
fn sql_null_and_json_null_stay_distinct() -> TestResult {
    let absent = encoded(Value::Json(None))?;
    let present = encoded(Value::Json(Some(Box::new(Json::Null))))?;
    assert_eq!(absent["sql_null"], json!(true));
    assert_eq!(present["sql_null"], json!(false));
    assert_eq!(absent["data"], present["data"]);
    assert_eq!(absent["type"], present["type"]);
    Ok(())
}

#[test]
fn an_empty_array_is_a_present_value() -> TestResult {
    assert_eq!(
        encoded(Value::Array(ArrayType::String, Some(Box::new(Vec::new()))))?,
        json!({
            "version": 1,
            "type": {"kind": "array", "element": {"kind": "text"}},
            "sql_null": false,
            "data": [],
        })
    );
    Ok(())
}

#[test]
fn array_elements_keep_their_own_null_flags() -> TestResult {
    let values = vec![text("%_"), Value::String(None)];
    assert_eq!(
        encoded(Value::Array(ArrayType::String, Some(Box::new(values))))?,
        json!({
            "version": 1,
            "type": {"kind": "array", "element": {"kind": "text"}},
            "sql_null": false,
            "data": [
                {"version": 1, "type": {"kind": "text"}, "sql_null": false, "data": "%_"},
                {"version": 1, "type": {"kind": "text"}, "sql_null": true, "data": Json::Null},
            ],
        })
    );
    Ok(())
}

#[test]
fn floats_travel_as_ieee_bits() -> TestResult {
    for (value, bits) in [
        (0.0f32, "00000000"),
        (-0.0f32, "80000000"),
        (f32::NAN, "7fc00000"),
        (f32::INFINITY, "7f800000"),
        (1.0f32, "3f800000"),
    ] {
        assert_eq!(encoded(Value::Float(Some(value)))?["data"], json!(bits));
    }
    for (value, bits) in [
        (-0.0f64, "8000000000000000"),
        (f64::NAN, "7ff8000000000000"),
        (f64::NEG_INFINITY, "fff0000000000000"),
        (0.1f64, "3fb999999999999a"),
    ] {
        assert_eq!(encoded(Value::Double(Some(value)))?["data"], json!(bits));
    }
    Ok(())
}

#[test]
fn a_nan_payload_survives_the_bit_pattern() -> TestResult {
    // Signalling NaN: the reason floats are not encoded as JSON numbers.
    let signalling = f32::from_bits(0x7f80_0001);
    assert!(signalling.is_nan());
    assert_eq!(
        encoded(Value::Float(Some(signalling)))?["data"],
        json!("7f800001")
    );
    Ok(())
}

#[test]
fn a_vector_is_a_list_of_bit_patterns() -> TestResult {
    let vector = Vector::from(vec![-0.0f32, 1.0f32]);
    assert_eq!(
        encoded(Value::Vector(Some(Box::new(vector))))?["data"],
        json!(["80000000", "3f800000"])
    );
    Ok(())
}

#[test]
fn integers_travel_as_canonical_decimal_text() -> TestResult {
    assert_eq!(encoded(Value::TinyInt(Some(-128)))?["data"], json!("-128"));
    assert_eq!(
        encoded(Value::SmallInt(Some(i16::MIN)))?["data"],
        json!("-32768")
    );
    assert_eq!(encoded(Value::Int(Some(0)))?["data"], json!("0"));
    assert_eq!(
        encoded(Value::BigInt(Some(i64::MIN)))?["data"],
        json!("-9223372036854775808")
    );
    assert_eq!(
        encoded(Value::BigUnsigned(Some(u64::MAX)))?["data"],
        json!("18446744073709551615")
    );
    assert_eq!(
        encoded(Value::Unsigned(Some(u32::MAX)))?["data"],
        json!("4294967295")
    );
    Ok(())
}

#[test]
fn a_decimal_keeps_its_trailing_scale() -> TestResult {
    for input in [
        "123.4500",
        "0.0000",
        "-0.0001",
        "79228162514264337593543950335",
    ] {
        let value = Decimal::from_str_exact(input)?;
        assert_eq!(
            encoded(Value::Decimal(Some(Box::new(value))))?["data"],
            json!(input)
        );
    }
    Ok(())
}

#[test]
fn bytes_and_mac_addresses_travel_as_integer_arrays() -> TestResult {
    assert_eq!(
        encoded(Value::Bytes(Some(Box::new(vec![0, 127, 255]))))?,
        json!({
            "version": 1,
            "type": {"kind": "bytes"},
            "sql_null": false,
            "data": [0, 127, 255],
        })
    );
    let mac = MacAddress::new([0x01, 0x02, 0x03, 0xfd, 0xfe, 0xff]);
    assert_eq!(
        encoded(Value::MacAddress(Some(Box::new(mac))))?["data"],
        json!([1, 2, 3, 253, 254, 255])
    );
    Ok(())
}

#[test]
fn text_and_char_carry_hostile_content_unchanged() -> TestResult {
    assert_eq!(
        encoded(text("O'Brien \" 雪\\\0"))?["data"],
        json!("O'Brien \" 雪\\\0")
    );
    assert_eq!(encoded(Value::Char(Some('雪')))?["data"], json!("雪"));
    Ok(())
}

#[test]
fn every_temporal_kind_matches_the_python_normalisation() -> TestResult {
    let naive = moment()?;
    let offset = FixedOffset::east_opt(5 * 3600 + 30 * 60).ok_or("unrepresentable offset")?;
    let fixed = naive
        .and_local_timezone(offset)
        .single()
        .ok_or("ambiguous local time")?;

    let cases: Vec<(Value, &str, &str)> = vec![
        (
            Value::ChronoDate(Some(Box::new(naive.date()))),
            "date",
            "2024-01-02",
        ),
        (
            Value::ChronoTime(Some(Box::new(naive.time()))),
            "time",
            "03:04:05.123456",
        ),
        (
            Value::ChronoDateTime(Some(Box::new(naive))),
            "datetime",
            "2024-01-02 03:04:05.123456",
        ),
        (
            Value::ChronoDateTimeUtc(Some(Box::new(naive.and_utc()))),
            "datetime_utc",
            "2024-01-02 03:04:05.123456 UTC",
        ),
        (
            Value::ChronoDateTimeWithTimeZone(Some(Box::new(fixed))),
            "datetime_fixed",
            "2024-01-02 03:04:05.123456 +05:30",
        ),
    ];
    for (value, kind, rendered) in cases {
        let document = encoded(value)?;
        assert_eq!(document["type"], json!({"kind": kind}), "{kind}");
        assert_eq!(document["data"], json!(rendered), "{kind}");
    }
    Ok(())
}

#[test]
fn a_local_datetime_keeps_its_kind_anywhere() -> TestResult {
    let local: DateTime<Local> = moment()?.and_utc().into();
    let document = encoded(Value::ChronoDateTimeLocal(Some(Box::new(local))))?;
    assert_eq!(document["type"], json!({"kind": "datetime_local"}));
    Ok(())
}

#[test]
fn temporal_text_reads_what_chrono_prints() -> TestResult {
    for (printed, normalized) in [
        ("2024-01-02", "2024-01-02"),
        ("03:04:05.123456", "03:04:05.123456"),
        // chrono prints three fractional digits when it can; Python needs six.
        ("03:04:05.123", "03:04:05.123000"),
        // An all-zero fraction is dropped, not padded.
        ("03:04:05.000000", "03:04:05"),
        (
            "2024-01-02 03:04:05.123456 UTC",
            "2024-01-02 03:04:05.123456+00:00",
        ),
        (
            "2024-01-02T03:04:05.123456Z",
            "2024-01-02 03:04:05.123456+00:00",
        ),
        (
            "2024-01-02 03:04:05.123456 +05:30",
            "2024-01-02 03:04:05.123456+05:30",
        ),
        (
            "2024-01-02 03:04:05.123456 -05:30",
            "2024-01-02 03:04:05.123456-05:30",
        ),
        ("2024-01-02T03:04:05", "2024-01-02 03:04:05"),
    ] {
        assert_eq!(temporal_text(printed)?, normalized, "{printed}");
    }
    assert!(temporal_text("03:04:05.123456789").is_err());
    Ok(())
}

#[test]
fn temporal_parsers_read_back_what_the_encoder_wrote() -> TestResult {
    let naive = moment()?;
    let offset = FixedOffset::east_opt(-8 * 3600).ok_or("unrepresentable offset")?;
    let fixed = naive
        .and_local_timezone(offset)
        .single()
        .ok_or("ambiguous local time")?;
    assert_eq!(parse_date(&naive.date().to_string())?, naive.date());
    assert_eq!(parse_time(&naive.time().to_string())?, naive.time());
    assert_eq!(parse_naive_datetime(&naive.to_string())?, naive);
    assert_eq!(
        parse_datetime_utc(&naive.and_utc().to_string())?,
        naive.and_utc()
    );
    assert_eq!(parse_datetime_fixed(&fixed.to_string())?, fixed);
    // chrono's own FromStr demands the `T` its Display never writes.
    assert!(naive.to_string().parse::<NaiveDateTime>().is_err());
    Ok(())
}

#[test]
fn a_datetime_kind_must_agree_with_its_offset() {
    assert!(parse_naive_datetime("2024-01-02 03:04:05+00:00").is_err());
    assert!(parse_datetime_fixed("2024-01-02 03:04:05").is_err());
    assert!(parse_datetime_utc("2024-01-02 03:04:05+05:30").is_err());
    assert!(parse_datetime_utc("2024-01-02 03:04:05 UTC").is_ok());
}

#[test]
fn an_enum_carries_its_qualified_identity() -> TestResult {
    let name = TypeName::new("State\" 雪").in_schema("fixture");
    let label = Tagged::from_enum(text("O'Brien 雪"), name.clone(), false);
    assert_eq!(
        label.encode_checked()?,
        json!({
            "version": 1,
            "type": {"kind": "enum", "name": "State\" 雪", "schema": "fixture"},
            "sql_null": false,
            "data": "O'Brien 雪",
        })
    );

    let values = vec![text("calm"), Value::String(None)];
    let array = Tagged::from_enum(
        Value::Array(ArrayType::String, Some(Box::new(values))),
        name,
        true,
    );
    let document = array.encode_checked()?;
    assert_eq!(
        document["type"],
        json!({
            "kind": "array",
            "element": {"kind": "enum", "name": "State\" 雪", "schema": "fixture"},
        })
    );
    // The element tag is the enum's, not the `text` the payload alone implies.
    assert_eq!(document["data"][0]["type"], document["type"]["element"]);
    assert_eq!(document["data"][1]["sql_null"], json!(true));
    Ok(())
}

#[test]
fn an_unqualified_enum_records_a_null_schema() -> TestResult {
    let label = Tagged::from_enum(text("calm"), TypeName::new("mood"), false);
    assert_eq!(
        label.encode_checked()?["type"],
        json!({"kind": "enum", "name": "mood", "schema": Json::Null})
    );
    Ok(())
}

#[test]
fn validation_rejects_what_the_python_format_rejects() {
    let malformed = [
        json!({"version": 2, "type": {"kind": "i32"}, "sql_null": true, "data": Json::Null}),
        json!({"version": 1, "type": {"kind": "i32"}, "sql_null": true, "data": "0"}),
        json!({"version": 1, "type": {"kind": "i32"}, "sql_null": false, "data": 0}),
        json!({"version": 1, "type": {"kind": "i8"}, "sql_null": false, "data": "128"}),
        json!({"version": 1, "type": {"kind": "i32"}, "sql_null": false, "data": "-0"}),
        json!({"version": 1, "type": {"kind": "f32"}, "sql_null": false, "data": "7FC00000"}),
        json!({"version": 1, "type": {"kind": "f32"}, "sql_null": false, "data": "7fc0000"}),
        json!({"version": 1, "type": {"kind": "char"}, "sql_null": false, "data": "ab"}),
        json!({"version": 1, "type": {"kind": "bytes"}, "sql_null": false, "data": [256]}),
        json!({"version": 1, "type": {"kind": "mac_address"}, "sql_null": false, "data": [1, 2]}),
        json!({"version": 1, "type": {"kind": "decimal"}, "sql_null": false, "data": "01.5"}),
        json!({"version": 1, "type": {"kind": "decimal"}, "sql_null": false, "data": "1."}),
        json!({"version": 1, "type": {"kind": "uuid"}, "sql_null": false,
               "data": "00000000-0000-0000-0000-00000000000A"}),
        json!({"version": 1, "type": {"kind": "ipnetwork"}, "sql_null": false, "data": "10.0.0.1"}),
        json!({"version": 1, "type": {"kind": "date"}, "sql_null": false, "data": "2024-1-2"}),
        json!({"version": 1, "type": {"kind": "unknown"}, "sql_null": true, "data": Json::Null}),
        json!({
            "version": 1,
            "type": {"kind": "array", "element": {"kind": "array", "element": {"kind": "i32"}}},
            "sql_null": true,
            "data": Json::Null,
        }),
        // An element whose tag differs from the one the array declares.
        json!({
            "version": 1,
            "type": {"kind": "array", "element": {"kind": "text"}},
            "sql_null": false,
            "data": [{"version": 1, "type": {"kind": "i32"}, "sql_null": false, "data": "1"}],
        }),
        // An enum name past PostgreSQL's identifier limit.
        json!({
            "version": 1,
            "type": {"kind": "enum", "name": "n".repeat(64), "schema": Json::Null},
            "sql_null": true,
            "data": Json::Null,
        }),
    ];
    for document in malformed {
        assert!(validate(&document).is_err(), "{document}");
    }
}

#[test]
fn validation_accepts_the_canonical_spellings() -> TestResult {
    let well_formed = [
        json!({"version": 1, "type": {"kind": "uuid"}, "sql_null": false,
               "data": "00000000-0000-0000-0000-000000000001"}),
        json!({"version": 1, "type": {"kind": "ipnetwork"}, "sql_null": false,
               "data": "10.0.0.1/24"}),
        json!({"version": 1, "type": {"kind": "json"}, "sql_null": false,
               "data": {"owner": "O'Brien 雪", "null": Json::Null}}),
        json!({"version": 1, "type": {"kind": "bool"}, "sql_null": false, "data": false}),
        json!({"version": 1, "type": {"kind": "vector"}, "sql_null": false,
               "data": ["80000000"]}),
    ];
    for document in well_formed {
        validate(&document)?;
    }
    Ok(())
}

#[test]
fn a_tag_names_its_kind() {
    assert_eq!(Tag::Scalar(ArrayType::Decimal).name(), "decimal");
    assert_eq!(Tag::Enum(TypeName::new("mood")).name(), "enum");
    assert_eq!(Tag::Scalar(ArrayType::Int).array().name(), "array");
}

#[test]
fn observations_take_the_shapes_the_python_effects_produce() {
    assert_eq!(observe::count(3), json!({"kind": "count", "value": 3}));
    assert_eq!(observe::unit(), json!({"kind": "unit"}));
    assert_eq!(observe::absent(), json!({"kind": "absent"}));
    assert_eq!(
        observe::transaction("t0"),
        json!({"kind": "transaction", "scope": "t0"})
    );
    assert_eq!(
        observe::tuple(vec![observe::absent()]),
        json!({"kind": "tuple", "items": [{"kind": "absent"}]})
    );
    assert_eq!(
        observe::error(&ObservedError::new(
            "DatabaseError",
            "relation does not exist",
            Some("42P01".to_owned()),
        )),
        json!({
            "kind": "error",
            "class": "DatabaseError",
            "cause": "relation does not exist",
            "sqlstate": "42P01",
        })
    );
}

#[test]
fn a_report_is_executed_until_a_step_fails() {
    let mut report = Report::new("0".repeat(64).as_str());
    assert!(report.is_empty());
    report.observed(
        "s0",
        "fetch",
        &["pgorm::pipeline::Pipeline::filter"],
        observe::count(1),
    );
    assert_eq!(report.status(), "executed");
    report.failed(
        "s1",
        "execute",
        &["pgorm::ConnectionTrait::execute_raw"],
        &ObservedError::new("DecodeError", "column 0", None),
    );
    assert_eq!(report.len(), 2);
    assert_eq!(report.status(), "error");

    let document = report.to_json();
    assert_eq!(document["status"], json!("error"));
    assert_eq!(document["builds"], json!(0));
    assert_eq!(document["cleanup_errors"], json!([]));
    assert_eq!(document["steps"][0]["status"], json!("observed"));
    assert_eq!(document["steps"][0]["operation"], json!("fetch"));
    assert_eq!(document["steps"][1]["operation"], json!("execute"));
    assert_eq!(
        document["steps"][0]["native_paths"],
        json!(["pgorm::pipeline::Pipeline::filter"])
    );
    assert_eq!(
        document["steps"][1]["observation"]["class"],
        json!("DecodeError")
    );
    assert_eq!(document["steps"][1]["observation"]["sqlstate"], Json::Null);
}

#[test]
fn redaction_removes_connection_secrets_from_diagnostics() -> TestResult {
    let config: pgorm::Config = "postgres://campaign:hunter2@localhost/subject".parse()?;
    let redactions = crate::Redactions::from_config(&config);
    assert_eq!(
        redactions.apply("role campaign could not authenticate with hunter2"),
        "role [redacted] could not authenticate with [redacted]"
    );
    Ok(())
}

#[test]
fn utc_and_fixed_offsets_round_trip_normalised() -> TestResult {
    let instant = moment()?.and_utc();
    let normalized = temporal_text(&instant.to_string())?;
    assert_eq!(normalized, "2024-01-02 03:04:05.123456+00:00");
    assert_eq!(parse_datetime_utc(&normalized)?, instant);
    assert_eq!(
        parse_datetime_fixed(&normalized)?.with_timezone(&Utc),
        instant
    );
    Ok(())
}

#[test]
fn the_copied_entity_keeps_its_hostile_enum() {
    use pgorm::{ColumnTrait, EntityTrait, QueryFilter, QueryTrait};

    let (sql, values) = entities::account::Entity::find()
        .filter(entities::account::Column::State.eq(entities::account::State::Busy))
        .build();
    assert!(sql.contains(r#""fixture"."accounts""#), "{sql}");
    // The enum name carries a quote and a non-ASCII character; both have to
    // reach the cast through the identifier quoter rather than as raw text.
    assert!(sql.contains(r#"AS fixture."State"" 雪""#), "{sql}");
    assert_eq!(values.0.len(), 1);
}

#[test]
fn the_copied_graphs_quote_their_given_aliases() {
    use pgorm::QueryTrait;

    let aliases = vec!["n\" 雪".to_owned()];
    let (sql, _) = entities::graphs::optional(&aliases).build();
    assert!(sql.contains(r#""n"" 雪""#), "{sql}");
    assert!(sql.contains("LEFT JOIN"), "{sql}");

    let (sql, _) = entities::graphs::required(&aliases).build();
    assert!(sql.contains("INNER JOIN"), "{sql}");

    let many = (0..6).map(|n| format!("n{n}")).collect::<Vec<_>>();
    let (sql, _) = entities::graphs::arity7(&many).build();
    assert_eq!(sql.matches("LEFT JOIN").count(), 6, "{sql}");
}
