use super::*;
use crate::oracle::{assert_eq, parsed_nodes};
use pg_query::protobuf::ConstrType;

fn table(constraint: IndexConstraint) -> String {
    Table::create(Glyph::Table)
        .col(ColumnDef::new(Glyph::Id).integer().not_null())
        .col(ColumnDef::new(Glyph::Aspect).integer().not_null())
        .col(ColumnDef::new(Glyph::Image).text())
        .index(constraint)
        .to_string()
}

/// The one key constraint in `sql` — `PRIMARY KEY` or `UNIQUE`, as against
/// the columns' `NOT NULL`s — as the parser read it.
fn constraint_node(sql: &str) -> serde_json::Value {
    let mut found: Vec<_> = parsed_nodes(sql, "Constraint")
        .into_iter()
        .filter(|node| {
            matches!(
                contype(node),
                ConstrType::ConstrPrimary | ConstrType::ConstrUnique
            )
        })
        .collect();
    assert_eq!(found.len(), 1, "exactly one key constraint in {sql}");
    found.remove(0)
}

fn names(list: &serde_json::Value) -> Vec<String> {
    list.as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item["String"]["sval"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn contype(node: &serde_json::Value) -> ConstrType {
    let code = node["contype"].as_i64().expect("a constraint type");
    ConstrType::try_from(code as i32).expect("a known constraint type")
}

// [spec:pgorm:req:sql.ddl.create-table+10/test]    each key an embedded constraint can declare
// renders as the table constraint it names, its key columns plain names
#[test]
fn every_key_renders_as_its_table_constraint() {
    let cases = [
        (
            IndexConstraint::primary_key(Glyph::Id).col(Glyph::Aspect),
            r#"PRIMARY KEY ("id", "aspect")"#,
            ConstrType::ConstrPrimary,
            false,
        ),
        (
            IndexConstraint::unique(Glyph::Aspect),
            r#"UNIQUE ("aspect")"#,
            ConstrType::ConstrUnique,
            false,
        ),
        (
            IndexConstraint::unique_nulls_not_distinct(Glyph::Image).col(Glyph::Aspect),
            r#"UNIQUE NULLS NOT DISTINCT ("image", "aspect")"#,
            ConstrType::ConstrUnique,
            true,
        ),
    ];

    for (constraint, rendered, kind, nulls_not_distinct) in cases {
        let sql = table(constraint.clone());
        assert_eq!(
            sql,
            [
                r#"CREATE TABLE "glyph" ( "id" integer NOT NULL, "aspect" integer NOT NULL,"#,
                &format!(r#""image" text, {rendered} )"#),
            ]
            .join(" ")
        );
        let node = constraint_node(&sql);
        assert_eq!(contype(&node), kind, "{sql}");
        assert_eq!(
            names(&node["keys"]),
            constraint
                .get_columns()
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>(),
            "{sql}"
        );
        assert_eq!(node["nulls_not_distinct"], nulls_not_distinct, "{sql}");
        assert_eq!(
            constraint.is_primary_key(),
            kind == ConstrType::ConstrPrimary
        );
        assert_eq!(constraint.is_unique_key(), kind == ConstrType::ConstrUnique);
        assert_eq!(constraint.is_nulls_not_distinct(), nulls_not_distinct);
    }
}

// [spec:pgorm:req:sql.ddl.create-table+10/test]    a name, INCLUDE and deferrability ride on
// any key, in the grammar's order
// [spec:pgorm:req:sql.ddl.deferrability+3/test]
#[test]
fn a_constraint_takes_a_name_include_and_deferrability() {
    let constraint = IndexConstraint::unique(Glyph::Aspect)
        .name(Name::runtime("glyph_aspect"))
        .include([Glyph::Image])
        .include([Glyph::Id])
        .deferrability(Deferrability::DeferrableInitiallyImmediate);
    let sql = table(constraint.clone());
    assert!(
        sql.ends_with(
            r#""image" text, CONSTRAINT "glyph_aspect" UNIQUE ("aspect") INCLUDE ("image", "id") DEFERRABLE INITIALLY IMMEDIATE )"#
        ),
        "{sql}"
    );
    let node = constraint_node(&sql);
    assert_eq!(node["conname"], "glyph_aspect");
    assert_eq!(names(&node["including"]), ["image", "id"]);
    assert_eq!(node["deferrable"], true);
    assert_eq!(node["initdeferred"], false, "{sql}");

    assert_eq!(
        constraint.get_name().map(|name| name.to_string()),
        Some("glyph_aspect".to_owned())
    );
    assert_eq!(constraint.get_include().len(), 2);
    assert_eq!(
        constraint.get_deferrability(),
        Some(Deferrability::DeferrableInitiallyImmediate)
    );

    // Unnamed, the constraint writes no CONSTRAINT clause and PostgreSQL
    // derives the name.
    let sql = table(IndexConstraint::primary_key(Glyph::Id));
    assert!(
        sql.ends_with(r#""image" text, PRIMARY KEY ("id") )"#),
        "{sql}"
    );
    assert_eq!(constraint_node(&sql)["conname"], "", "{sql}");
}

// [spec:pgorm:req:sql.ddl.create-table+10/test]    constraints embed in order, by value, after
// the columns
#[test]
fn constraints_embed_in_order() {
    let key = IndexConstraint::primary_key(Glyph::Id);
    let statement = Table::create(Glyph::Table)
        .col(ColumnDef::new(Glyph::Id).integer().not_null())
        .col(ColumnDef::new(Glyph::Aspect).integer())
        .index(key.clone())
        .index(IndexConstraint::unique(Glyph::Aspect))
        .to_owned();
    assert_eq!(
        statement.to_string(),
        r#"CREATE TABLE "glyph" ( "id" integer NOT NULL, "aspect" integer, PRIMARY KEY ("id"), UNIQUE ("aspect") )"#
    );
    let read: Vec<bool> = statement
        .get_indexes()
        .iter()
        .map(IndexConstraint::is_primary_key)
        .collect();
    assert_eq!(read, [true, false]);
    assert!(key.is_primary_key());
}
