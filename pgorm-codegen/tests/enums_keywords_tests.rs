//! Generated active enums and identifier hygiene (Rust keyword escaping,
//! snake_case field names, preserved DB column names).

mod common;

use common::*;
use pgorm_codegen::WithSerde;
use pgorm_query::{Alias, ColumnDef, ColumnType, Table};

// [spec:pgorm:sem:codegen.entity.enums+1/test]    every discovered enum lands in
// one `pgorm_active_enums.rs`, alphabetically by enum name
#[test]
fn active_enums_are_generated_into_one_alphabetical_file() {
    let generated = generate(
        vec![
            table_with(
                "zoo",
                vec![
                    serial_pk("id"),
                    enum_col("kind", "zebra_kind", &["plains", "mountain"]),
                ],
            ),
            table_with(
                "orchard",
                vec![
                    serial_pk("id"),
                    enum_col("kind", "apple_kind", &["fuji", "gala"]),
                ],
            ),
        ],
        Opts::default(),
    );

    let enums = generated.file("pgorm_active_enums.rs");
    assert_eq!(
        generated
            .names()
            .iter()
            .filter(|n| n.contains("active_enums"))
            .count(),
        1
    );
    assert!(position_of(enums, "pub enum AppleKind") < position_of(enums, "pub enum ZebraKind"));
}

// [spec:pgorm:sem:codegen.entity.enums+1/test]    each enum gets the base derives,
// the `#[pgorm(rs_type, db_type, enum_name)]` attribute, extra attributes, an
// UpperCamelCase name and one `string_value` per variant
#[test]
fn active_enum_derives_attributes_and_string_values() {
    let generated = generate(
        vec![table_with(
            "tea_pairing",
            vec![
                serial_pk("id"),
                enum_col("tea", "tea_kind", &["EverydayTea", "BreakfastTea"]),
            ],
        )],
        Opts {
            with_copy_enums: true,
            with_serde: WithSerde::Serialize,
            enum_extra_derives: vec!["Hash".to_owned()],
            enum_extra_attributes: vec!["non_exhaustive".to_owned()],
            ..Default::default()
        },
    );

    assert_contains(
        generated.file("pgorm_active_enums.rs"),
        r#"#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Copy, Serialize, Hash)]
           #[pgorm(rs_type = "String", db_type = "Enum", enum_name = "tea_kind")]
           #[non_exhaustive]
           pub enum TeaKind {
               #[pgorm(string_value = "EverydayTea")]
               EverydayTea,
               #[pgorm(string_value = "BreakfastTea")]
               BreakfastTea,
           }"#,
    );
}

// [spec:pgorm:sem:codegen.entity.enums+1/test]    variant naming: a leading digit
// gets an underscore prefix, an empty UpperCamelCase falls back to per-character
// encoding (ASCII as `U<hex>`, multi-byte verbatim), everything else is plain
// UpperCamelCase
#[test]
fn active_enum_variant_naming_digits_punctuation_multibyte() {
    let generated = generate(
        vec![table_with(
            "doc",
            vec![
                serial_pk("id"),
                enum_col("ty", "ty", &["Question", "A-B-C", "3D", "/", "//", "你好"]),
            ],
        )],
        Opts::default(),
    );

    assert_contains(
        generated.file("pgorm_active_enums.rs"),
        r#"pub enum Ty {
               #[pgorm(string_value = "Question")]
               Question,
               #[pgorm(string_value = "A-B-C")]
               ABC,
               #[pgorm(string_value = "3D")]
               _3D,
               #[pgorm(string_value = "/")]
               U002F,
               #[pgorm(string_value = "//")]
               U002FU002F,
               #[pgorm(string_value = "你好")]
               你好,
           }"#,
    );
}

// [spec:pgorm:sem:codegen.entity.enums+1/test]    entity files import each enum
// they use, and the expanded `ColumnTrait::def` renders enum columns as
// `<EnumName>::db_type()`
#[test]
fn entity_files_import_enums_and_render_db_type() {
    let generated = generate(
        vec![table_with(
            "tea_pairing",
            vec![
                serial_pk("id"),
                enum_col("first_tea", "tea_kind", &["black", "green"]),
                enum_col("second_tea", "tea_kind", &["black", "green"]),
            ],
        )],
        expanded(),
    );
    let file = generated.file("tea_pairing.rs");

    // deduplicated: one import for the two columns of the same enum
    assert_eq!(
        file.matches("use super :: pgorm_active_enums :: TeaKind ;")
            .count(),
        1,
        "{file}"
    );
    assert_contains(file, "Self::FirstTea => TeaKind::db_type().def(),");
    assert_contains(file, "Self::SecondTea => TeaKind::db_type().def(),");
}

// [spec:pgorm:sem:codegen.entity.keywords+1/test]    strict / reserved Rust
// keywords are emitted as raw identifiers
#[test]
fn rust_keywords_in_column_names_become_raw_identifiers() {
    let generated = generate(
        vec![table_with(
            "sample",
            vec![
                serial_pk("id"),
                typed("type", ColumnType::Integer),
                typed("typeof", ColumnType::Integer),
                typed("match", ColumnType::Integer),
                typed("async", ColumnType::Integer),
                typed("plain", ColumnType::Integer),
            ],
        )],
        expanded(),
    );
    let sample = generated.file("sample.rs");

    assert_contains(
        sample,
        "pub struct Model {
            pub id: i32,
            pub r#type: i32,
            pub r#typeof: i32,
            pub r#match: i32,
            pub r#async: i32,
            pub plain: i32,
        }",
    );
    // the camel-case Column variants are not keywords, so they stay bare
    assert_contains(
        sample,
        "pub enum Column { Id, Type, Typeof, Match, Async, Plain, }",
    );
}

// [spec:pgorm:sem:codegen.entity.keywords+1/test]    `crate`, `self` and `Self`
// cannot be raw identifiers, so they get a trailing underscore instead
#[test]
fn crate_and_self_keywords_get_a_trailing_underscore() {
    let generated = generate(
        vec![table_with(
            "sample",
            vec![
                serial_pk("id"),
                typed("crate", ColumnType::Integer),
                typed("self", ColumnType::Integer),
            ],
        )],
        expanded(),
    );
    let sample = generated.file("sample.rs");

    // field names: `crate` -> `crate_`, `self` -> `self_`
    assert_contains(
        sample,
        "pub struct Model { pub id: i32, pub crate_: i32, pub self_: i32, }",
    );
    // the camel-case form of `self` is `Self`, which is escaped the same way
    assert_contains(sample, "pub enum Column { Id, Crate, Self_, }");
}

// [spec:pgorm:sem:codegen.entity.keywords+1/test]    module names and table idents
// are escaped too
#[test]
fn keyword_table_names_escaped_in_index_and_prelude() {
    let generated = generate(
        vec![
            table_with("type", vec![serial_pk("id")]),
            table_with("crate", vec![serial_pk("id")]),
        ],
        Opts::default(),
    );

    let index: Vec<&str> = generated
        .file("mod.rs")
        .lines()
        .filter(|l| l.starts_with("pub mod") && !l.contains("prelude"))
        .collect();
    assert_eq!(index, ["pub mod crate_ ;", "pub mod r#type ;"]);

    let prelude: Vec<&str> = generated
        .file("prelude.rs")
        .lines()
        .filter(|l| l.starts_with("pub use"))
        .collect();
    assert_eq!(
        prelude,
        [
            "pub use super :: crate_ :: Entity as Crate ;",
            "pub use super :: r#type :: Entity as Type ;",
        ]
    );
}

// [spec:pgorm:sem:codegen.entity.keywords+1/test]    field names are the snake_case
// of the column name; when that differs from the raw DB name the DB name is
// preserved by a `column_name` attribute in both formats
#[test]
fn non_snake_case_names_preserved_by_column_name() {
    let schema = || {
        vec![
            Table::create(Alias::new("cake"))
                .col(serial_pk("id"))
                .col(
                    ColumnDef::new_with_type(Alias::new("bakedAt"), ColumnType::Integer)
                        .not_null()
                        .to_owned(),
                )
                .to_owned(),
        ]
    };

    let compact = generate(schema(), Opts::default());
    assert_contains(
        compact.file("cake.rs"),
        r#"#[pgorm(column_name = "bakedAt")] pub baked_at: i32,"#,
    );

    let expanded = generate(schema(), expanded());
    assert_contains(expanded.file("cake.rs"), "pub baked_at: i32,");
    assert_contains(
        expanded.file("cake.rs"),
        r#"pub enum Column { Id, #[pgorm(column_name = "bakedAt")] BakedAt, }"#,
    );
}

// [spec:pgorm:req:codegen.entity.collisions/test]    two tables whose names
// derive one module name are refused by both source names: one file would be
// written over the other and the module declared twice
#[test]
fn colliding_table_names_are_refused() {
    assert_transform_error(
        vec![
            table_with("CakeFilling", vec![serial_pk("id")]),
            table_with("cake_filling", vec![serial_pk("id")]),
        ],
        "tables `CakeFilling` and `cake_filling` both generate the module name `cake_filling`",
    );
}

// [spec:pgorm:req:codegen.entity.collisions/test]    two columns of one table
// whose names derive one field name are refused, naming the table and both
// columns
#[test]
fn colliding_column_names_are_refused() {
    assert_transform_error(
        vec![table_with(
            "cake",
            vec![
                serial_pk("id"),
                typed("bakedAt", ColumnType::Integer),
                typed("baked_at", ColumnType::Integer),
            ],
        )],
        "table `cake` columns `bakedAt` and `baked_at` both generate the field name `baked_at`",
    );
}

// [spec:pgorm:req:codegen.entity.collisions/test]    two enums that derive one
// type name, and two values of one enum that derive one variant name, are
// refused the same way
#[test]
fn colliding_enum_names_and_variants_are_refused() {
    assert_transform_error(
        vec![table_with(
            "cake",
            vec![
                serial_pk("id"),
                enum_col("kind", "media type", &["photo"]),
                enum_col("sort", "media_type", &["video"]),
            ],
        )],
        "enums `media type` and `media_type` both generate the type name `MediaType`",
    );

    assert_transform_error(
        vec![table_with(
            "cake",
            vec![
                serial_pk("id"),
                enum_col("kind", "tea", &["Earl Grey", "earl_grey"]),
            ],
        )],
        "enum `tea` values `Earl Grey` and `earl_grey` both generate the variant name `EarlGrey`",
    );
}
