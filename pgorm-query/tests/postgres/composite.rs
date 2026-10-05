use super::*;
use crate::oracle::{assert_eq, parsed_nodes};
use pgorm_query::extension::Type;

fn n(name: &str) -> Name {
    Name::runtime(name)
}

/// The one `CompositeTypeStmt` in `sql`, as PostgreSQL's parser read it.
fn composite(sql: &str) -> serde_json::Value {
    let mut found = parsed_nodes(sql, "CompositeTypeStmt");
    assert_eq!(found.len(), 1, "exactly one composite type in {sql}");
    found.remove(0)
}

/// Each attribute's name, type name parts and collation name parts, as the
/// parser read them.
fn attributes(statement: &serde_json::Value) -> Vec<(String, Vec<String>, Vec<String>)> {
    let parts = |list: &serde_json::Value| -> Vec<String> {
        list.as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|part| {
                part["String"]["sval"]
                    .as_str()
                    .expect("a name part")
                    .to_owned()
            })
            .collect()
    };
    statement["coldeflist"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|def| {
            let def = &def["ColumnDef"];
            (
                def["colname"]
                    .as_str()
                    .expect("an attribute name")
                    .to_owned(),
                parts(&def["type_name"]["names"]),
                parts(&def["coll_clause"]["collname"]),
            )
        })
        .collect()
}

// [spec:pgorm:req:sql.ddl.type-composite+1/test]    each attribute is a name, a type as a column
// writes it, and a collation when it has one, in the order given
#[test]
fn a_composite_lists_its_attributes() {
    let sql = Type::create((n("geo"), n("address")))
        .attribute_collated(n("Street"), ColumnType::Text, (n("pg_catalog"), n("C")))
        .attribute(n("no"), ColumnType::Integer)
        .attribute(n("area"), ColumnType::Decimal(Some((10, 2))))
        .attribute(
            n("tags"),
            ColumnType::Array(std::sync::Arc::new(ColumnType::Text)),
        )
        .attribute(n("kind"), ColumnType::named("mood"))
        .to_string();
    assert_eq!(
        sql,
        [
            r#"CREATE TYPE "geo"."address" AS ("Street" text COLLATE "pg_catalog"."C","#,
            r#""no" integer, "area" decimal(10, 2), "tags" text[], "kind" mood)"#,
        ]
        .join(" ")
    );
    let read = composite(&sql);
    assert_eq!(read["typevar"]["schemaname"], "geo");
    assert_eq!(read["typevar"]["relname"], "address");
    let s = |text: &str| text.to_owned();
    assert_eq!(
        attributes(&read),
        [
            (s("Street"), vec![s("text")], vec![s("pg_catalog"), s("C")]),
            (s("no"), vec![s("pg_catalog"), s("int4")], vec![]),
            (s("area"), vec![s("pg_catalog"), s("numeric")], vec![]),
            (s("tags"), vec![s("text")], vec![]),
            (s("kind"), vec![s("mood")], vec![]),
        ]
    );
}

// [spec:pgorm:req:sql.ddl.type-composite+1/test]    the empty composite keeps its parentheses
#[test]
fn an_empty_composite_is_a_type() {
    let sql = Type::create(n("nothing")).as_composite().to_string();
    assert_eq!(sql, r#"CREATE TYPE "nothing" AS ()"#);
    assert!(attributes(&composite(&sql)).is_empty());
    crate::oracle::assert_rejected(r#"CREATE TYPE "nothing" AS"#);
}

// [spec:pgorm:req:sql.ddl.type-composite+1/test]    a type is an enumeration or a composite, and
// choosing one kind replaces the other's list
// [spec:pgorm:req:sql.ddl.type-enum+7/test]
#[test]
fn a_type_is_one_kind() {
    let sql = Type::create(n("t"))
        .values(["a", "b"])
        .attribute(n("x"), ColumnType::Integer)
        .to_string();
    assert_eq!(sql, r#"CREATE TYPE "t" AS ("x" integer)"#);

    let sql = Type::create(n("t"))
        .attribute(n("x"), ColumnType::Integer)
        .values(["a"])
        .to_string();
    assert_eq!(sql, r#"CREATE TYPE "t" AS ENUM ('a')"#);

    let sql = Type::create(n("t"))
        .values(["a"])
        .as_composite()
        .to_string();
    assert_eq!(sql, r#"CREATE TYPE "t" AS ()"#);

    let sql = Type::create(n("t"))
        .attribute(n("x"), ColumnType::Integer)
        .as_enum()
        .to_string();
    assert_eq!(sql, r#"CREATE TYPE "t" AS ENUM ()"#);

    // Re-choosing the kind a type already has keeps its list.
    let sql = Type::create(n("t"))
        .attribute(n("x"), ColumnType::Integer)
        .as_composite()
        .attribute(n("y"), ColumnType::Text)
        .to_string();
    assert_eq!(sql, r#"CREATE TYPE "t" AS ("x" integer, "y" text)"#);
    let sql = Type::create(n("t"))
        .values(["a"])
        .as_enum()
        .values(["b"])
        .to_string();
    assert_eq!(sql, r#"CREATE TYPE "t" AS ENUM ('a', 'b')"#);
}

// [spec:pgorm:req:sql.ddl.type-composite+1/test]    an attribute carries no column spec, because
// the grammar refuses every one of them there
#[test]
fn an_attribute_is_not_a_column() {
    for refused in [
        r#"CREATE TYPE "t" AS ("a" integer NOT NULL)"#,
        r#"CREATE TYPE "t" AS ("a" integer DEFAULT 1)"#,
        r#"CREATE TYPE "t" AS ("a" integer PRIMARY KEY)"#,
        r#"CREATE TYPE "t" AS ("a" integer CHECK ("a" > 0))"#,
        r#"CREATE TYPE "t" AS ("a" integer GENERATED ALWAYS AS IDENTITY)"#,
    ] {
        crate::oracle::assert_rejected(refused);
    }
}

// [spec:pgorm:req:sql.ddl.type-composite+1/test]    a composite binds nothing, so its two
// renderings agree
#[test]
fn a_composite_binds_nothing() {
    let statement = Type::create(n("t"))
        .attribute(n("x"), ColumnType::Integer)
        .to_owned();
    let (sql, values) = statement.build();
    assert_eq!(sql, statement.to_string());
    assert!(values.0.is_empty());
}
