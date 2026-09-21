//! Column type mapping, the unsupported-type policy, and how primary keys
//! surface in each format.

mod common;

use common::*;
use pgorm_codegen::{Column, EntityTransformer, Error};
use pgorm_query::{ColumnDef, ColumnType, Name, StringLen, Table, TypeName};
use proc_macro2::{TokenStream, TokenTree};
use quote::quote;
use std::sync::Arc;

// [spec:pgorm:sem:codegen.entity.types+3/test]    `Column::get_rs_type` follows
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
                typed("c_named", ColumnType::named("citext")),
                typed("c_small", ColumnType::SmallInteger),
                typed("c_int", ColumnType::Integer),
                typed("c_big", ColumnType::BigInteger),
                typed("c_json", ColumnType::Json),
                typed("c_jsonb", ColumnType::JsonBinary),
                typed("c_decimal", ColumnType::Decimal(None)),
                typed("c_money", ColumnType::Money),
                typed("c_uuid", ColumnType::Uuid),
                typed("c_bytea", ColumnType::Bytea),
                typed("c_bool", ColumnType::Boolean),
                enum_col("c_enum", "tea_kind", &["black", "green"]),
                ColumnDef::new(Name::runtime("c_array"))
                    .array(ColumnType::Integer)
                    .not_null()
                    .to_owned(),
                ColumnDef::new(Name::runtime("c_nested_array"))
                    .array(ColumnType::Array(Arc::new(ColumnType::Integer)))
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
        ("c_named", "String"),
        ("c_small", "i16"),
        ("c_int", "i32"),
        ("c_big", "i64"),
        ("c_json", "Json"),
        ("c_jsonb", "Json"),
        ("c_decimal", "Decimal"),
        ("c_money", "Decimal"),
        ("c_uuid", "Uuid"),
        ("c_bytea", "Vec<u8>"),
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

// [spec:pgorm:sem:codegen.entity.types+3/test]    `Float` and `Double` also map
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
                ColumnDef::new(Name::runtime("ratios"))
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

// [spec:pgorm:sem:codegen.entity.types.datetime+2/test]    date/time columns map
// to the prelude's temporal aliases, and to nothing else — there is no option
// selecting a different set, so the mapping is the whole surface
#[test]
fn date_time_columns_map_to_prelude_aliases() {
    let generated = generate(
        vec![table_with(
            "moment",
            vec![
                serial_pk("id"),
                typed("d", ColumnType::Date),
                typed("t", ColumnType::Time),
                typed("ts", ColumnType::Timestamp),
                typed("tstz", ColumnType::TimestampWithTimeZone),
                typed("many", ColumnType::Array(Arc::new(ColumnType::Timestamp))),
            ],
        )],
        Opts::default(),
    );
    let moment = generated.file("moment.rs");
    for (field, rust_type) in [
        ("d", "Date"),
        ("t", "Time"),
        ("ts", "DateTime"),
        ("tstz", "DateTimeWithTimeZone"),
        ("many", "Vec<DateTime>"),
    ] {
        assert_contains(moment, &format!("pub {field}: {rust_type},"));
    }
    // No crate in the workspace defines a `TimeDate` family, so generated source
    // naming one cannot compile against pgorm. Nothing may reach these spellings.
    for absent in [
        "TimeDate",
        "TimeTime",
        "TimeDateTime",
        "TimeDateTimeWithTimeZone",
    ] {
        assert!(
            !moment.contains(absent),
            "generated source names `{absent}`, which no crate defines:\n{moment}"
        );
    }
}

/// A type name Postgres accepts and Rust source does not: it holds a double
/// quote and a space, so pasting it between two quote characters ends the
/// attribute's literal in the middle of the name.
const HOSTILE_NAME: &str = "ev\"il type";

/// The compact attribute codegen emits for [`HOSTILE_NAME`], exactly as it
/// appears in the generated file. `hostile_entity` below carries this same
/// text through the real derive, so the two halves of the round trip are
/// pinned to one spelling.
const HOSTILE_ATTR: &str = r#"#[pgorm(column_type = "named(\"ev\\\"il type\")")]"#;

/// A generated entity written out by hand, carrying [`HOSTILE_ATTR`] verbatim.
/// That it compiles at all is half the claim; `Column::Odd`'s definition is the
/// other half.
mod hostile_entity {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
    #[pgorm(table_name = "sample")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        #[pgorm(column_type = "named(\"ev\\\"il type\")")]
        pub odd: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// The value of the sole string literal in an attribute — the `LitStr` the
/// derive reads out of `column_type = "..."` before parsing it as tokens.
fn attribute_string_value(attr: &str) -> String {
    let mut literal = None;
    fn walk(stream: TokenStream, found: &mut Option<String>) {
        for tree in stream {
            match tree {
                TokenTree::Group(group) => walk(group.stream(), found),
                TokenTree::Literal(lit) => *found = Some(lit.to_string()),
                _ => {}
            }
        }
    }
    walk(
        attr.parse().expect("the attribute should lex as Rust"),
        &mut literal,
    );
    let literal = literal.expect("the attribute should carry a literal");
    syn::parse_str::<syn::LitStr>(&literal)
        .expect("the literal should be a string")
        .value()
}

// [spec:pgorm:sem:codegen.entity.compact.attrs+3/test]    the compact attribute
// carries a named type's name as a rendered string literal, so a name holding a
// quote arrives at the derive as the name that was described rather than as
// tokens that escaped the literal
#[test]
fn hostile_named_type_survives_the_compact_attribute() {
    let generated = generate(
        vec![table_with(
            "sample",
            vec![
                serial_pk("id"),
                typed("odd", ColumnType::named(HOSTILE_NAME)),
            ],
        )],
        Opts::default(),
    );

    assert_contains(
        generated.file("sample.rs"),
        &format!("{HOSTILE_ATTR} pub odd: String,"),
    );

    // The derive's own read of that attribute, step for step: the `LitStr`'s
    // value, parsed as tokens, spliced after `ColumnType::`.
    let spliced: TokenStream = syn::parse_str(&attribute_string_value(HOSTILE_ATTR))
        .expect("the attribute value should parse as tokens");
    let expected = quote!(pgorm::prelude::ColumnType::named("ev\"il type")).to_string();
    assert_eq!(
        quote!(pgorm::prelude::ColumnType::#spliced).to_string(),
        expected,
    );

    // The control the rest of this test rests on: pasting the same name
    // between two quote characters, as `format!` would, does not produce the
    // same program. It fails in one of two ways, and both are here because
    // only one of them is loud.
    //
    // This name ends the literal at its own quote and leaves a trailing `")`
    // that never closes, so the derive cannot parse the attribute at all — a
    // compile error inside generated source, pointing at an attribute nobody
    // wrote by hand.
    assert!(
        syn::parse_str::<TokenStream>(&format!("named(\"{HOSTILE_NAME}\")")).is_err(),
        "the pasted spelling of {HOSTILE_NAME:?} should not parse",
    );
    // A name carrying a comment opener parses perfectly well, and names a
    // different type than the one described — the tail of the name is
    // commented out, and nothing anywhere reports it.
    let truncating = "x\") //";
    let pasted: TokenStream = syn::parse_str(&format!("named(\"{truncating}\")"))
        .expect("the pasted spelling of a comment-bearing name parses");
    assert_eq!(
        quote!(pgorm::prelude::ColumnType::#pasted).to_string(),
        quote!(pgorm::prelude::ColumnType::named("x")).to_string(),
        "the pasted spelling should silently lose the rest of the name",
    );
    // Rendered as a literal, that same name survives whole.
    let rendered: TokenStream = syn::parse_str(&format!(
        "named({})",
        proc_macro2::Literal::string(truncating)
    ))
    .expect("the rendered spelling parses");
    assert_eq!(
        quote!(pgorm::prelude::ColumnType::#rendered).to_string(),
        quote!(pgorm::prelude::ColumnType::named("x\") //")).to_string(),
    );
}

// [spec:pgorm:sem:codegen.entity.compact.attrs+3/test]    and the entity the
// derive builds from it names the type that was described, character for
// character
#[test]
fn derived_entity_names_the_hostile_type_exactly() {
    use pgorm::entity::prelude::ColumnTrait;

    assert_eq!(
        hostile_entity::Column::Odd.def().get_column_type(),
        &ColumnType::named(HOSTILE_NAME),
    );
}

// [spec:pgorm:sem:codegen.entity.types+3/test]    the expanded writer spells the
// same name the same way, so the two emission paths agree on a hostile name as
// they do on a benign one
#[test]
fn hostile_named_type_survives_the_expanded_column_def() {
    let generated = generate(
        vec![table_with(
            "sample",
            vec![
                serial_pk("id"),
                typed("odd", ColumnType::named(HOSTILE_NAME)),
            ],
        )],
        expanded(),
    );

    assert_contains(
        generated.file("sample.rs"),
        r#"ColumnType::named("ev\"il type").def()"#,
    );
}

// [spec:pgorm:req:codegen.entity.types.unsupported+2/test]    a named type the
// generated `ColumnType::named("..")` could not rebuild — schema-qualified, an
// array, or a type expression — is refused by name rather than respelled as a
// different type
#[test]
fn column_conversion_rejects_an_unrespellable_named_type() {
    for (col_type, spelled) in [
        (
            ColumnType::Named(TypeName::new(Name::runtime("status")).schema(Name::runtime("app"))),
            "app.status",
        ),
        (
            ColumnType::Named(TypeName::new(Name::runtime("citext")).array()),
            "citext[]",
        ),
        (
            ColumnType::Named(TypeName::raw("numeric(12, 2)")),
            "numeric(12, 2)",
        ),
    ] {
        let col_def = ColumnDef::new_with_type(Name::runtime("odd"), col_type).to_owned();
        match Column::try_from(&col_def) {
            Err(Error::TransformError(msg)) => assert_eq!(
                msg,
                format!(
                    "column `odd`: named column type `{spelled}` is not supported by codegen; \
                     only a bare type name survives the generated `ColumnType::named(\"..\")`"
                )
            ),
            other => panic!("expected a TransformError, got {other:?}"),
        }
    }
}

// [spec:pgorm:req:codegen.entity.types.unsupported+2/test]    a type outside the
// mapping fails the whole run with a `TransformError` naming table, column and
// type — no placeholder code, and no panic
#[test]
fn transform_rejects_column_type_outside_mapping() {
    let unsupported = EntityTransformer::transform(vec![table_with(
        "device",
        vec![serial_pk("id"), typed("address", ColumnType::Inet)],
    )]);

    match unsupported {
        Err(Error::TransformError(msg)) => assert_eq!(
            msg,
            "table `device` column `address`: column type Inet is not supported by codegen"
        ),
        other => panic!("expected a TransformError, got {other:?}"),
    }
}

// [spec:pgorm:req:codegen.entity.types.unsupported+2/test]    the check sits at
// `Column` construction, so the writer never meets an unmapped type; `Array`
// element types are checked through
#[test]
fn column_conversion_rejects_unsupported_type() {
    for col_type in [
        ColumnType::Inet,
        ColumnType::Array(Arc::new(ColumnType::Inet)),
    ] {
        let col_def = ColumnDef::new_with_type(Name::runtime("address"), col_type).to_owned();
        match Column::try_from(&col_def) {
            Err(Error::TransformError(msg)) => assert_eq!(
                msg,
                "column `address`: column type Inet is not supported by codegen"
            ),
            other => panic!("expected a TransformError, got {other:?}"),
        }
    }
}

// [spec:pgorm:sem:codegen.entity.pk/test]    the expanded `ValueType` is the PK
// column's Rust type, or a tuple for a composite key
#[test]
fn expanded_pk_value_type_is_type_or_tuple() {
    let single = generate(vec![cake()], expanded());
    assert_contains(single.file("cake.rs"), "type ValueType = i32;");

    let composite = generate(vec![cake(), cake_filling(), filling()], expanded());
    assert_contains(
        composite.file("cake_filling.rs"),
        "type ValueType = (i32, i32);",
    );

    let text_key = generate(
        vec![table_with(
            "setting",
            vec![
                ColumnDef::new_with_type(Name::runtime("key"), ColumnType::Text)
                    .not_null()
                    .primary_key()
                    .to_owned(),
            ],
        )],
        expanded(),
    );
    assert_contains(text_key.file("setting.rs"), "type ValueType = String;");
}

// [spec:pgorm:sem:codegen.entity.pk/test]    `auto_increment()` is true when any
// column of the table is auto-increment, not just a primary-key column
#[test]
fn expanded_pk_auto_increment_looks_at_every_column() {
    let generated = generate(
        vec![
            Table::create(Name::runtime("ticket"))
                .col(
                    ColumnDef::new_with_type(Name::runtime("code"), ColumnType::Text)
                        .not_null()
                        .primary_key()
                        .to_owned(),
                )
                // not part of the primary key, yet it flips `auto_increment()`
                .col(
                    ColumnDef::new(Name::runtime("seq"))
                        .integer()
                        .not_null()
                        .auto_increment()
                        .to_owned(),
                )
                .to_owned(),
        ],
        expanded(),
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
                ColumnDef::new_with_type(Name::runtime("code"), ColumnType::Text)
                    .not_null()
                    .primary_key()
                    .to_owned(),
            ],
        )],
        expanded(),
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

    let composite = generate(vec![cake(), cake_filling(), filling()], Opts::default());
    assert_contains(
        composite.file("cake_filling.rs"),
        "#[pgorm(primary_key, auto_increment = false)] pub cake_id: i32,
         #[pgorm(primary_key, auto_increment = false)] pub filling_id: i32,",
    );
}
