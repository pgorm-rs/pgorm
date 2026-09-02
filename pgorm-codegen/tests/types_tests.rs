//! Column type mapping, the date/time crate selection, the unsupported-type
//! policy, and how primary keys surface in each format.

mod common;

use common::*;
use pgorm_codegen::{Column, DateTimeCrate};
use pgorm_query::{Alias, ColumnDef, ColumnType, RcOrArc, StringLen, Table};

// [spec:pgorm:sem:codegen.entity.types+1/test]    `Column::get_rs_type` follows
// the mapping table, wrapping nullable columns in `Option`
#[test]
fn column_rust_types_follow_the_mapping_table() {
    let generated = generate(
        vec![table_with(
            "sample",
            vec![
                serial_pk("id"),
                typed("c_char", ColumnType::Char(Some(1))),
                typed("c_string", ColumnType::String(StringLen::N(10))),
                typed("c_text", ColumnType::Text),
                typed("c_custom", ColumnType::custom("citext")),
                typed("c_tiny", ColumnType::TinyInteger),
                typed("c_small", ColumnType::SmallInteger),
                typed("c_int", ColumnType::Integer),
                typed("c_big", ColumnType::BigInteger),
                typed("c_unsigned", ColumnType::Unsigned),
                typed("c_big_unsigned", ColumnType::BigUnsigned),
                typed("c_json", ColumnType::Json),
                typed("c_jsonb", ColumnType::JsonBinary),
                typed("c_decimal", ColumnType::Decimal(None)),
                typed("c_money", ColumnType::Money(None)),
                typed("c_uuid", ColumnType::Uuid),
                typed("c_binary", ColumnType::Binary(4)),
                typed("c_varbinary", ColumnType::VarBinary(StringLen::Max)),
                typed("c_blob", ColumnType::Blob),
                typed("c_bool", ColumnType::Boolean),
                enum_col("c_enum", "tea_kind", &["black", "green"]),
                ColumnDef::new(Alias::new("c_array"))
                    .array(ColumnType::Integer)
                    .not_null()
                    .to_owned(),
                ColumnDef::new(Alias::new("c_nested_array"))
                    .array(ColumnType::Array(RcOrArc::new(ColumnType::Integer)))
                    .not_null()
                    .to_owned(),
                // nullable: the same mapping, wrapped in `Option`
                typed_null("c_null_int", ColumnType::Integer),
                typed_null("c_null_uuid", ColumnType::Uuid),
            ],
        )],
        Opts::default(),
    );
    let sample = generated.file("sample.rs");

    for (field, rust_type) in [
        ("c_char", "String"),
        ("c_string", "String"),
        ("c_text", "String"),
        ("c_custom", "String"),
        ("c_tiny", "i8"),
        ("c_small", "i16"),
        ("c_int", "i32"),
        ("c_big", "i64"),
        ("c_unsigned", "u32"),
        ("c_big_unsigned", "u64"),
        ("c_json", "Json"),
        ("c_jsonb", "Json"),
        ("c_decimal", "Decimal"),
        ("c_money", "Decimal"),
        ("c_uuid", "Uuid"),
        ("c_binary", "Vec<u8>"),
        ("c_varbinary", "Vec<u8>"),
        ("c_blob", "Vec<u8>"),
        ("c_bool", "bool"),
        ("c_enum", "TeaKind"),
        ("c_array", "Vec<i32>"),
        ("c_nested_array", "Vec<Vec<i32>>"),
        ("c_null_int", "Option<i32>"),
        ("c_null_uuid", "Option<Uuid>"),
    ] {
        assert_contains(sample, &format!("pub {field}: {rust_type}"));
    }
}

// [spec:pgorm:sem:codegen.entity.types+1/test]    `Float` and `Double` also map
// to `f32` / `f64`, and either one suppresses the Model's `Eq` derive —
// recursively through `Array`
#[test]
fn float_and_double_columns_suppress_the_eq_derive() {
    let no_floats = generate(
        vec![table_with("sample", vec![serial_pk("id")])],
        Opts::default(),
    );
    assert_contains(
        no_floats.file("sample.rs"),
        "#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]",
    );

    let with_float = generate(
        vec![table_with(
            "sample",
            vec![serial_pk("id"), typed("ratio", ColumnType::Float)],
        )],
        Opts::default(),
    );
    assert_contains(with_float.file("sample.rs"), "pub ratio: f32,");
    assert_contains(
        with_float.file("sample.rs"),
        "#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]",
    );

    let with_double = generate(
        vec![table_with(
            "sample",
            vec![serial_pk("id"), typed("ratio", ColumnType::Double)],
        )],
        Opts::default(),
    );
    assert_contains(with_double.file("sample.rs"), "pub ratio: f64,");
    assert_contains(
        with_double.file("sample.rs"),
        "#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]",
    );

    // through an array element type
    let with_float_array = generate(
        vec![table_with(
            "sample",
            vec![
                serial_pk("id"),
                ColumnDef::new(Alias::new("ratios"))
                    .array(ColumnType::Float)
                    .not_null()
                    .to_owned(),
            ],
        )],
        Opts::default(),
    );
    assert_contains(
        with_float_array.file("sample.rs"),
        "#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]",
    );
}

// [spec:pgorm:sem:codegen.entity.types.datetime/test]    `DateTimeCrate` picks
// the date/time field types
#[test]
fn date_time_crate_selects_date_time_field_types() {
    let schema = || {
        vec![table_with(
            "moment",
            vec![
                serial_pk("id"),
                typed("d", ColumnType::Date),
                typed("t", ColumnType::Time),
                typed("dt", ColumnType::DateTime),
                typed("ts", ColumnType::Timestamp),
                typed("tstz", ColumnType::TimestampWithTimeZone),
            ],
        )]
    };

    let chrono = generate(schema(), Opts::default());
    for (field, rust_type) in [
        ("d", "Date"),
        ("t", "Time"),
        ("dt", "DateTime"),
        ("ts", "DateTimeUtc"),
        ("tstz", "DateTimeWithTimeZone"),
    ] {
        assert_contains(
            chrono.file("moment.rs"),
            &format!("pub {field}: {rust_type},"),
        );
    }

    let time = generate(
        schema(),
        Opts {
            date_time_crate: DateTimeCrate::Time,
            ..Default::default()
        },
    );
    for (field, rust_type) in [
        ("d", "TimeDate"),
        ("t", "TimeTime"),
        ("dt", "TimeDateTime"),
        ("ts", "TimeDateTime"),
        ("tstz", "TimeDateTimeWithTimeZone"),
    ] {
        assert_contains(
            time.file("moment.rs"),
            &format!("pub {field}: {rust_type},"),
        );
    }
}

// [spec:pgorm:req:codegen.entity.types.unsupported/test]    a type outside the
// mapping aborts the generation run by panic rather than emitting a placeholder
#[test]
#[should_panic(expected = "column type Inet is not supported by codegen")]
fn generation_panics_on_column_type_outside_mapping() {
    let _ = generate(
        vec![table_with(
            "device",
            vec![serial_pk("id"), typed("address", ColumnType::Inet)],
        )],
        Opts::default(),
    );
}

// [spec:pgorm:req:codegen.entity.types.unsupported/test]    the expanded-format
// `ColumnDef` writer panics on its wildcard arm for unmapped types
#[test]
#[should_panic(expected = "not implemented")]
fn expanded_column_def_writer_panics_on_unsupported_type() {
    let column: Column =
        (&ColumnDef::new_with_type(Alias::new("address"), ColumnType::Inet).to_owned()).into();
    let _ = column.get_def();
}

// [spec:pgorm:sem:codegen.entity.pk/test]    the expanded `ValueType` is the PK
// column's Rust type, or a tuple for a composite key
#[test]
fn expanded_pk_value_type_is_type_or_tuple() {
    let single = generate(vec![cake()], Opts::expanded());
    assert_contains(single.file("cake.rs"), "type ValueType = i32;");

    let composite = generate(vec![cake_filling()], Opts::expanded());
    assert_contains(
        composite.file("cake_filling.rs"),
        "type ValueType = (i32, i32);",
    );

    let text_key = generate(
        vec![table_with(
            "setting",
            vec![
                ColumnDef::new_with_type(Alias::new("key"), ColumnType::Text)
                    .not_null()
                    .primary_key()
                    .to_owned(),
            ],
        )],
        Opts::expanded(),
    );
    assert_contains(text_key.file("setting.rs"), "type ValueType = String;");
}

// [spec:pgorm:sem:codegen.entity.pk/test]    `auto_increment()` is true when any
// column of the table is auto-increment, not just a primary-key column
#[test]
fn expanded_pk_auto_increment_looks_at_every_column() {
    let generated = generate(
        vec![
            Table::create()
                .table(Alias::new("ticket"))
                .col(
                    ColumnDef::new_with_type(Alias::new("code"), ColumnType::Text)
                        .not_null()
                        .primary_key()
                        .to_owned(),
                )
                // not part of the primary key, yet it flips `auto_increment()`
                .col(
                    ColumnDef::new(Alias::new("seq"))
                        .integer()
                        .not_null()
                        .auto_increment()
                        .to_owned(),
                )
                .to_owned(),
        ],
        Opts::expanded(),
    );

    assert_contains(
        generated.file("ticket.rs"),
        "impl PrimaryKeyTrait for PrimaryKey {
            type ValueType = String;
            fn auto_increment() -> bool { true }
        }",
    );

    let no_auto = generate(
        vec![table_with(
            "ticket",
            vec![
                ColumnDef::new_with_type(Alias::new("code"), ColumnType::Text)
                    .not_null()
                    .primary_key()
                    .to_owned(),
            ],
        )],
        Opts::expanded(),
    );
    assert_contains(
        no_auto.file("ticket.rs"),
        "fn auto_increment() -> bool { false }",
    );
}

// [spec:pgorm:sem:codegen.entity.pk/test]    in the compact format the same facts
// surface as the `primary_key` / `auto_increment = false` field attributes
#[test]
fn compact_primary_key_facts_surface_as_field_attributes() {
    let auto = generate(vec![cake()], Opts::default());
    assert_contains(auto.file("cake.rs"), "#[pgorm(primary_key)] pub id: i32,");
    assert_not_contains(auto.file("cake.rs"), "auto_increment = false");

    let composite = generate(vec![cake_filling()], Opts::default());
    assert_contains(
        composite.file("cake_filling.rs"),
        "#[pgorm(primary_key, auto_increment = false)] pub cake_id: i32,
         #[pgorm(primary_key, auto_increment = false)] pub filling_id: i32,",
    );
}
