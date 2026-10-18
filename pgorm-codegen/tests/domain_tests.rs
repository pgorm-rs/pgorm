//! Domains: `CREATE DOMAIN` read by the DDL bridge, the columns typed by one,
//! and the entity both writers generate for them — each field the Rust type
//! of the domain's base, the domain's name carried in the column's definition.

mod common;

use common::*;
use pgorm::{ColumnTrait, ColumnTypeTrait, Iterable, StaticName};
use pgorm_codegen::sql_schema::{entities_from_sql, parse_schema};
use pgorm_codegen::{Error, WriterOutput};
use pgorm_query::{
    ColumnType, Domain, Expr, Name, StringLen, TableCreateStatement,
    extension::{RangeDefinition, Type},
};
use std::sync::Arc;

const SCHEMA: &str = include_str!("sql/domain/schema.sql");

/// The files the bridge generates for [`SCHEMA`], compiled here as well as
/// compared: the derives accept every domain the writer emits. The live round
/// trip through them is `tests/codegen_domain_tests.rs` at the workspace
/// root.
#[path = "sql/domain/pgorm_active_enums.rs"]
mod pgorm_active_enums;

#[path = "sql/domain/pgorm_range_types.rs"]
mod pgorm_range_types;

#[path = "sql/domain/stock_line.rs"]
mod stock_line;

/// The same entity in the expanded format, included rather than declared so
/// that rustfmt leaves the fixture in the shape the writer emits.
mod expanded_line {
    include!("sql/domain/expanded/stock_line.rs");
}

/// The columns [`SCHEMA`] declares nullable.
const NULLABLE: [&str; 4] = ["qty", "moods", "labels", "span"];

fn domain(name: &str, schema: Option<&str>, base: ColumnType) -> ColumnType {
    ColumnType::Domain {
        name: Name::runtime(name),
        schema: schema.map(Name::runtime),
        base: Arc::new(base),
    }
}

fn mood() -> ColumnType {
    ColumnType::Enum {
        name: Name::runtime("mood"),
        schema: None,
        variants: ["sad", "calm", "glad"].map(Name::runtime).to_vec(),
    }
}

fn quantity() -> ColumnType {
    domain("quantity", None, ColumnType::Integer)
}

fn code() -> ColumnType {
    domain("code", None, ColumnType::String(StringLen::N(8)))
}

fn cheerful() -> ColumnType {
    domain("cheerful", None, mood())
}

fn floatrange() -> ColumnType {
    ColumnType::CreatedRange {
        name: Name::runtime("floatrange"),
        schema: None,
        subtype: Arc::new(ColumnType::Double),
    }
}

/// The column types of [`SCHEMA`]'s table, in column order.
fn described() -> Vec<(&'static str, ColumnType)> {
    vec![
        ("id", ColumnType::Integer),
        ("qty", quantity()),
        ("small", domain("small_quantity", None, quantity())),
        ("code", code()),
        ("codes", ColumnType::Array(Arc::new(code()))),
        ("mood", cheerful()),
        ("moods", ColumnType::Array(Arc::new(cheerful()))),
        (
            "labels",
            domain(
                "labels",
                None,
                ColumnType::Array(Arc::new(ColumnType::Text)),
            ),
        ),
        ("span", domain("span", None, floatrange())),
        (
            "amount",
            domain(
                "amount",
                Some("billing"),
                ColumnType::Decimal(Some((12, 2))),
            ),
        ),
    ]
}

/// Token text with the trailing commas rustfmt writes into a fixture and the
/// writer does not, so the two compare as the same program.
fn flat(src: &str) -> String {
    norm(src).replace(" , }", " }").replace(" , )", " )")
}

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

// [spec:pgorm:sem:codegen.ddl.types+9/test]    a column named by a domain the file creates
// reads as `ColumnType::Domain`, carrying the domain's identity and its base read as a
// column's type is — another domain, an enum, an array and a range type among them —
// through an array as on its own
// [spec:pgorm:sem:codegen.ddl.objects+9/test]
#[test]
fn the_bridge_reads_a_domain_column() {
    let statements = parse_schema(SCHEMA).expect("the schema parses");
    let columns: Vec<_> = statements[0]
        .get_columns()
        .iter()
        .map(|column| column.get_column_type().cloned())
        .collect();
    let expected: Vec<_> = described()
        .into_iter()
        .map(|(_, col_type)| Some(col_type))
        .collect();
    assert_eq!(columns, expected);
}

// [spec:pgorm:sem:codegen.ddl.objects+9/test]    a domain's COLLATE, DEFAULT, NOT NULL, NULL
// and CHECKs are read and set aside: the entity a domain generates is the one its bare
// declaration does
#[test]
fn domain_constraints_are_set_aside() {
    let bare = "CREATE DOMAIN d AS text; CREATE TABLE t (id integer PRIMARY KEY, c d NOT NULL);";
    let dressed = "CREATE DOMAIN d AS text COLLATE \"C\" DEFAULT 'x' NOT NULL \
                   CONSTRAINT filled CHECK (VALUE <> '') CHECK (length(VALUE) < 9); \
                   CREATE TABLE t (id integer PRIMARY KEY, c d NOT NULL);";
    let nullable = "CREATE DOMAIN d AS text NULL; \
                    CREATE TABLE t (id integer PRIMARY KEY, c d NOT NULL);";
    let generate = |sql| files(entities_from_sql(sql, Opts::default()).expect("generates")).files;
    assert_eq!(generate(dressed), generate(bare));
    assert_eq!(generate(nullable), generate(bare));
}

// [spec:pgorm:req:codegen.ddl.unsupported+16/test]    a constraint PostgreSQL refuses on a
// domain is named, as are a serial base, a base the file does not declare and a domain
// column under a qualification the domain was not declared with
#[test]
fn refused_domain_shapes_are_named() {
    for (sql, what) in [
        (
            "CREATE DOMAIN d AS integer UNIQUE;",
            "a UNIQUE constraint on domain `d`",
        ),
        (
            "CREATE DOMAIN d AS integer PRIMARY KEY;",
            "a PRIMARY KEY on domain `d`",
        ),
        (
            "CREATE DOMAIN d AS integer REFERENCES t (id);",
            "a REFERENCES clause on domain `d`",
        ),
        (
            "CREATE DOMAIN d AS integer GENERATED ALWAYS AS (1) STORED;",
            "a GENERATED clause on domain `d`",
        ),
        (
            "CREATE DOMAIN d AS integer GENERATED ALWAYS AS IDENTITY;",
            "an identity clause on domain `d`",
        ),
        (
            "CREATE DOMAIN d AS integer CHECK (VALUE > 0) NO INHERIT;",
            "a NO INHERIT constraint on domain `d`",
        ),
        (
            "CREATE DOMAIN d AS integer CHECK (VALUE > 0) DEFERRABLE;",
            "a deferrable constraint on domain `d`",
        ),
        (
            "CREATE DOMAIN d AS integer CHECK (VALUE > 0) NOT ENFORCED;",
            "a NOT ENFORCED constraint on domain `d`",
        ),
        (
            "CREATE DOMAIN d AS serial;",
            "a serial type as the base type of domain `d`",
        ),
        (
            "CREATE DOMAIN d AS mood;",
            "type `mood` on the base type of domain `d`",
        ),
        (
            "CREATE DOMAIN a.b.d AS integer;",
            "a catalog-qualified type name in CREATE DOMAIN",
        ),
    ] {
        assert_eq!(
            error(sql),
            format!("unsupported DDL: {what} at statement 1"),
            "{sql}"
        );
    }

    assert_eq!(
        error(
            "CREATE DOMAIN billing.amount AS integer; \
             CREATE TABLE t (id integer PRIMARY KEY, a amount);"
        ),
        "statement 2: type `amount` on column `t`.`a` is not declared; a same-named domain \
         under a different qualification does not resolve it"
    );
    assert_eq!(
        error("CREATE TYPE d AS ENUM ('a'); CREATE DOMAIN d AS integer;"),
        "statement 2: type `d` is declared twice"
    );
}

// [spec:pgorm:req:codegen.entity.types.unsupported+8/test]    an array of a domain over an
// array, and an array of a domain over a range type a schema created, are refused by name;
// a domain over either alone generates
#[test]
fn unholdable_domain_arrays_are_named() {
    assert_eq!(
        error(
            "CREATE DOMAIN labels AS text[]; \
             CREATE TABLE t (id integer PRIMARY KEY, a labels[]);"
        ),
        "table `t` column `a`: an array of a domain over an array is not supported by \
         codegen; an entity field holds one dimension"
    );
    assert_eq!(
        error(
            "CREATE TYPE floatrange AS RANGE (SUBTYPE = float8); \
             CREATE DOMAIN span AS floatrange; \
             CREATE TABLE t (id integer PRIMARY KEY, a span[]);"
        ),
        "table `t` column `a`: an array of range type `floatrange` is not supported by \
         codegen; the newtype a created range generates has no array conversion"
    );
}

// [spec:pgorm:sem:codegen.entity.types+9/test]    a domain column is a field of its base's
// Rust type, an array of one a `Vec` of it, an enum or a created range's newtype included
// [spec:pgorm:sem:codegen.entity.compact.attrs+8/test]    the compact writer names every
// domain column's type, the domain and its base spelled in full
#[test]
fn domain_columns_generate_a_compiling_entity() {
    let generated =
        files(entities_from_sql(SCHEMA, Opts::default()).expect("the schema generates"));
    let written = flat(generated.file("stock_line.rs"));
    assert!(
        written.contains(&flat(include_str!("sql/domain/stock_line.rs"))),
        "{written}"
    );
    assert!(
        norm(generated.file("pgorm_active_enums.rs"))
            .contains(&norm(include_str!("sql/domain/pgorm_active_enums.rs")))
    );
    assert!(
        norm(generated.file("pgorm_range_types.rs"))
            .contains(&norm(include_str!("sql/domain/pgorm_range_types.rs")))
    );

    for (column, (name, col_type)) in stock_line::Column::iter().zip(described()) {
        assert_eq!(column.as_str(), name);
        let expected = match NULLABLE.contains(&name) {
            true => col_type.def().null(),
            false => col_type.def(),
        };
        assert_eq!(column.def(), expected, "{name}");
    }
}

// [spec:pgorm:sem:codegen.entity.types+9/test]    the expanded writer spells each domain in
// full, and defines every column as the compact derive does
#[test]
fn the_expanded_writer_spells_each_domain() {
    let generated = files(entities_from_sql(SCHEMA, expanded()).expect("the schema generates"));
    let written = flat(generated.file("stock_line.rs"));
    assert!(
        written.contains(&flat(include_str!("sql/domain/expanded/stock_line.rs"))),
        "{written}"
    );

    for (expanded, compact) in expanded_line::Column::iter().zip(stock_line::Column::iter()) {
        assert_eq!(expanded.def(), compact.def(), "{}", compact.as_str());
    }
}

/// [`SCHEMA`]'s types and table, built with pgorm-query rather than written
/// as text: the enum, the range type, each domain with its constraints, and
/// the table whose columns they type.
fn built() -> (Vec<String>, TableCreateStatement) {
    let n = Name::runtime;
    let types = vec![
        Type::create(n("mood"))
            .values(["sad", "calm", "glad"])
            .to_string(),
        Type::create(n("floatrange"))
            .as_range(RangeDefinition::new(ColumnType::Double))
            .to_string(),
    ];
    let mut statements = types;
    statements.extend([
        Domain::create(n("quantity"), ColumnType::Integer)
            .not_null()
            .default(1)
            .check(Expr::domain_value().gt(0))
            .to_string(),
        Domain::create(n("small_quantity"), quantity())
            .check(Expr::domain_value().lt(100))
            .to_string(),
        Domain::create(n("code"), ColumnType::String(StringLen::N(8)))
            .collate(n("C"))
            .to_string(),
        Domain::create(n("cheerful"), mood())
            .check(Expr::domain_value().ne("sad"))
            .to_string(),
        Domain::create(n("labels"), ColumnType::Array(Arc::new(ColumnType::Text))).to_string(),
        Domain::create(n("span"), floatrange()).to_string(),
        Domain::create(
            (n("billing"), n("amount")),
            ColumnType::Decimal(Some((12, 2))),
        )
        .to_string(),
    ]);
    let columns = described()
        .into_iter()
        .map(|(name, col_type)| match NULLABLE.contains(&name) {
            true => typed_null(name, col_type),
            false => typed(name, col_type),
        })
        .collect();
    (statements, keyed_with("stock_line", &["id"], columns))
}

// [spec:pgorm:def:codegen.ddl+3/test]    domains and a table of domain columns built with
// pgorm-query and rendered generate, read back, the entity the statement itself generates
#[test]
fn rendered_domain_columns_read_back_whole() {
    let (statements, table) = built();
    let text = statements
        .iter()
        .map(|statement| format!("{statement};"))
        .chain([format!("{table};")])
        .collect::<Vec<_>>()
        .join("\n");
    for expanded_format in [false, true] {
        let opts = || Opts {
            expanded_format,
            ..Default::default()
        };
        assert_eq!(
            files(entities_from_sql(&text, opts()).expect("the rendering generates")).files,
            generate(vec![table.clone()], opts()).files,
        );
    }
}
