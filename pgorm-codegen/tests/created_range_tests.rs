//! Range types a schema creates: `CREATE TYPE ... AS RANGE` read by the DDL
//! bridge, its columns typed by it, and the newtype each one generates.

mod common;

use common::*;
use pgorm::{ColumnTrait, ColumnTypeTrait, EntityTrait, QueryTrait};
use pgorm_codegen::sql_schema::{entities_from_sql, parse_schema};
use pgorm_codegen::{Error, WriterOutput};
use pgorm_query::{ColumnType, Name};
use std::sync::Arc;

const SCHEMA: &str = include_str!("sql/created_range/schema.sql");

/// The two files the bridge generates for [`SCHEMA`], compiled here as well
/// as compared: the derives accept what the writer emits. The live round trip
/// through them is `tests/created_range_tests.rs` at the workspace root.
#[path = "sql/created_range/pgorm_range_types.rs"]
mod pgorm_range_types;

#[path = "sql/created_range/measurement.rs"]
mod measurement;

fn files(output: WriterOutput) -> Generated {
    Generated {
        files: output
            .files
            .into_iter()
            .map(|file| (file.name, file.content))
            .collect(),
    }
}

#[track_caller]
fn error(sql: &str) -> String {
    match entities_from_sql(sql, Opts::default()) {
        Err(Error::TransformError(message)) => message,
        Err(other) => panic!("expected a TransformError, got {other:?}"),
        Ok(_) => panic!("expected an error, got generated entities"),
    }
}

fn created(name: &str, schema: Option<&str>, subtype: ColumnType) -> ColumnType {
    ColumnType::CreatedRange {
        name: Name::runtime(name),
        schema: schema.map(Name::runtime),
        subtype: Arc::new(subtype),
    }
}

// [spec:pgorm:sem:codegen.ddl.objects+7/test]    a range type the file creates
// types the columns naming it, by its full identity and with its subtype read
// as a column's type is
// [spec:pgorm:sem:codegen.ddl.types+6/test]
#[test]
fn a_created_range_types_its_columns() {
    let statements = parse_schema(SCHEMA).expect("the schema parses");
    let columns: Vec<_> = statements[0]
        .get_columns()
        .iter()
        .map(|column| column.get_column_type().cloned())
        .collect();
    assert_eq!(
        columns,
        [
            Some(ColumnType::Integer),
            Some(created("floatrange", None, ColumnType::Double)),
            Some(created("slot", Some("booking"), ColumnType::Integer)),
            Some(created("textrange", None, ColumnType::Text)),
        ]
    );
}

// [spec:pgorm:sem:codegen.entity.types+5/test]    one newtype per range type,
// named in UpperCamelCase and deriving `DeriveCreatedRange` under its name and
// schema, `Eq` but for a float subtype; the entity's fields name the newtypes
// [spec:pgorm:req:codegen.entity.files+1/test]    `pgorm_range_types.rs` is
// written and declared beside the entities
// [spec:pgorm:sem:codegen.entity.imports+1/test]    each entity imports the
// newtypes its columns name
#[test]
fn a_created_range_generates_a_compiling_newtype() {
    let generated =
        files(entities_from_sql(SCHEMA, Opts::default()).expect("the schema generates"));
    assert_eq!(
        generated.names(),
        [
            "measurement.rs",
            "mod.rs",
            "prelude.rs",
            "pgorm_range_types.rs"
        ]
    );
    assert_contains(generated.file("mod.rs"), "pub mod pgorm_range_types;");
    for (file, fixture) in [
        (
            "pgorm_range_types.rs",
            include_str!("sql/created_range/pgorm_range_types.rs"),
        ),
        (
            "measurement.rs",
            include_str!("sql/created_range/measurement.rs"),
        ),
    ] {
        let written = norm(generated.file(file)).replace(", )", ")");
        assert!(written.contains(&norm(fixture)), "{file}: {written}");
    }

    assert_eq!(
        measurement::Column::Slot.def(),
        created("slot", Some("booking"), ColumnType::Integer)
            .def()
            .null()
    );
    assert_eq!(
        <pgorm_range_types::Floatrange as pgorm::CreatedRange>::name().to_sql_string(),
        "floatrange"
    );
    assert_eq!(
        measurement::Entity::find().build().0,
        r#"SELECT "measurement"."id", "measurement"."span", "measurement"."slot", "measurement"."label" FROM "measurement""#
    );
}

// [spec:pgorm:sem:codegen.entity.types+5/test]    the expanded writer spells
// a created range column's type as the derive does
#[test]
fn the_expanded_writer_names_the_created_range() {
    let generated = files(entities_from_sql(SCHEMA, expanded()).expect("the schema generates"));
    assert_contains(
        generated.file("measurement.rs"),
        r#"Self::Slot => ColumnType::CreatedRange {
            name: Name::runtime("slot"),
            schema: Some(Name::runtime("booking")),
            subtype: Arc::new(ColumnType::Integer),
        }
        .def()
        .null(),"#,
    );
    assert_contains(
        generated.file("measurement.rs"),
        "pub span: Floatrange, pub slot: Option<Slot>, pub label: Textrange,",
    );
}

// [spec:pgorm:req:codegen.ddl.unsupported+11/test]    what the bridge cannot
// carry out of a range type is named: a subtype no newtype can hold, an array
// of a created range, an option PostgreSQL does not have, a range with no
// subtype, a serial subtype, and a type declared twice
// [spec:pgorm:req:codegen.entity.types.unsupported+5/test]
#[test]
fn what_a_created_range_cannot_carry_is_named() {
    assert_eq!(
        error(
            "CREATE TYPE flags AS RANGE (SUBTYPE = bool); CREATE TABLE t (id int PRIMARY KEY, f flags);"
        ),
        "table `t` column `f`: range type `flags` over Boolean is not supported by codegen; \
         a created range's subtype is one pgorm's `RangeSubtype` covers"
    );
    assert_eq!(
        error(
            "CREATE TYPE floatrange AS RANGE (SUBTYPE = float8); CREATE TABLE t (id int PRIMARY KEY, f floatrange[]);"
        ),
        "table `t` column `f`: an array of range type `floatrange` is not supported by codegen; \
         the newtype a created range generates has no array conversion"
    );
    assert_eq!(
        error("CREATE TYPE floatrange AS RANGE (SUBTYPE = float8, FLAVOUR = sweet);"),
        "unsupported DDL: option `flavour` on range type `floatrange` at statement 1"
    );
    assert_eq!(
        error("CREATE TYPE floatrange AS RANGE (SUBTYPE_DIFF = float8mi);"),
        "statement 1: range type `floatrange` has no SUBTYPE"
    );
    assert_eq!(
        error("CREATE TYPE serials AS RANGE (SUBTYPE = serial);"),
        "unsupported DDL: a serial type as the SUBTYPE of range type `serials` at statement 1"
    );
    assert_eq!(
        error("CREATE TYPE span AS ENUM ('a'); CREATE TYPE span AS RANGE (SUBTYPE = int4);"),
        "statement 2: type `span` is declared twice"
    );
    assert_eq!(
        error(
            "CREATE TYPE app.slot AS RANGE (SUBTYPE = int4); CREATE TABLE t (id int PRIMARY KEY, s slot);"
        ),
        "statement 2: type `slot` on column `t`.`s` is not declared; a same-named range type \
         under a different qualification does not resolve it"
    );
    assert_eq!(
        error(
            "CREATE TYPE a.slot AS RANGE (SUBTYPE = int4); CREATE TYPE b.slot AS RANGE (SUBTYPE = int4); \
               CREATE TABLE t (id int PRIMARY KEY, x a.slot, y b.slot);"
        ),
        "range type `slot` is used under two qualifications; the generated Rust newtype can carry only one"
    );
}

// [spec:pgorm:sem:codegen.ddl.objects+7/test]    the options that do not change
// a row's value are read and set aside, and a range over an enum the file
// declared reads its subtype as that enum, which no newtype holds
#[test]
fn range_options_are_read_and_set_aside() {
    let statements = parse_schema(
        "CREATE TYPE textrange AS RANGE (SUBTYPE = text, SUBTYPE_OPCLASS = text_pattern_ops, \
         COLLATION = \"C\", MULTIRANGE_TYPE_NAME = texts); \
         CREATE TABLE t (id int PRIMARY KEY, r textrange);",
    )
    .expect("the schema parses");
    assert_eq!(
        statements[0].get_columns()[1].get_column_type(),
        Some(&created("textrange", None, ColumnType::Text))
    );
    assert!(
        error(
            "CREATE TYPE mood AS ENUM ('sad', 'happy'); \
             CREATE TYPE moodrange AS RANGE (SUBTYPE = mood); \
             CREATE TABLE t (id int PRIMARY KEY, r moodrange);"
        )
        .starts_with("table `t` column `r`: range type `moodrange` over Enum")
    );
}
