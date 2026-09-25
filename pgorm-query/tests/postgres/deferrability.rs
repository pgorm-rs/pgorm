use super::*;
use crate::oracle::{assert_eq, parsed_nodes};
use pg_query::protobuf::ConstrType;

const STATES: [(Deferrability, &str); 3] = [
    (Deferrability::NotDeferrable, "NOT DEFERRABLE"),
    (
        Deferrability::DeferrableInitiallyImmediate,
        "DEFERRABLE INITIALLY IMMEDIATE",
    ),
    (
        Deferrability::DeferrableInitiallyDeferred,
        "DEFERRABLE INITIALLY DEFERRED",
    ),
];

/// The `contype` of every `Constraint` node in `sql`, in order.
fn constraint_types(sql: &str) -> Vec<ConstrType> {
    parsed_nodes(sql, "Constraint")
        .iter()
        .map(|constraint| {
            let contype = constraint["contype"].as_i64().expect("a constraint type");
            let contype = <i32 as TryFrom<i64>>::try_from(contype).expect("an enum value");
            ConstrType::try_from(contype).expect("a known constraint type")
        })
        .collect()
}

/// The raw parser's reading of a column constraint's deferrability: an
/// attribute node after the constraint it qualifies, one per keyword pair.
fn column_attributes(deferrability: Deferrability) -> Vec<ConstrType> {
    match deferrability {
        Deferrability::NotDeferrable => vec![ConstrType::ConstrAttrNotDeferrable],
        Deferrability::DeferrableInitiallyImmediate => vec![
            ConstrType::ConstrAttrDeferrable,
            ConstrType::ConstrAttrImmediate,
        ],
        Deferrability::DeferrableInitiallyDeferred => vec![
            ConstrType::ConstrAttrDeferrable,
            ConstrType::ConstrAttrDeferred,
        ],
    }
}

// [spec:pgorm:req:sql.ddl.deferrability/test]    a column's unique and primary keys carry the
// clause directly after their keyword, and the parser attaches it to them
// [spec:pgorm:req:sql.ddl.column-def+6/test]
#[test]
fn a_column_key_carries_its_deferrability() {
    for (deferrability, text) in STATES {
        let unique = Table::create(Glyph::Table)
            .col(
                ColumnDef::new(Glyph::Aspect)
                    .integer()
                    .unique_key_deferrability(deferrability)
                    .not_null(),
            )
            .to_string();
        assert_eq!(
            unique,
            format!(r#"CREATE TABLE "glyph" ( "aspect" integer UNIQUE {text} NOT NULL )"#)
        );
        let mut expected = vec![ConstrType::ConstrUnique];
        expected.extend(column_attributes(deferrability));
        expected.push(ConstrType::ConstrNotnull);
        assert_eq!(constraint_types(&unique), expected, "{unique}");

        let primary = Table::create(Glyph::Table)
            .col(
                ColumnDef::new(Glyph::Id)
                    .integer()
                    .primary_key_deferrability(deferrability),
            )
            .to_string();
        assert_eq!(
            primary,
            format!(r#"CREATE TABLE "glyph" ( "id" integer PRIMARY KEY {text} )"#)
        );
        let mut expected = vec![ConstrType::ConstrPrimary];
        expected.extend(column_attributes(deferrability));
        assert_eq!(constraint_types(&primary), expected, "{primary}");
    }

    // Without the clause the keys render exactly as they always have.
    assert_eq!(
        Table::create(Glyph::Table)
            .col(ColumnDef::new(Glyph::Id).integer().primary_key())
            .col(ColumnDef::new(Glyph::Aspect).integer().unique_key())
            .to_string(),
        r#"CREATE TABLE "glyph" ( "id" integer PRIMARY KEY, "aspect" integer UNIQUE )"#
    );
}

// [spec:pgorm:req:sql.ddl.deferrability/test]    a table-level unique or primary-key constraint
// carries the clause after its column list and INCLUDE
// [spec:pgorm:req:sql.ddl.create-table+9/test]
#[test]
fn a_table_constraint_carries_its_deferrability() {
    for (deferrability, text) in STATES {
        let sql = Table::create(Glyph::Table)
            .col(ColumnDef::new(Glyph::Id).integer())
            .col(ColumnDef::new(Glyph::Aspect).integer())
            .col(ColumnDef::new(Glyph::Image).text())
            .primary_key(
                Index::create(Glyph::Table, Glyph::Id)
                    .to_owned()
                    .deferrability(deferrability),
            )
            .index(
                Index::create(Glyph::Table, Glyph::Aspect)
                    .name(Name::runtime("glyph_aspect"))
                    .unique()
                    .nulls_not_distinct()
                    .include([Glyph::Image])
                    .to_owned()
                    .deferrability(deferrability),
            )
            .to_string();
        assert_eq!(
            sql,
            [
                r#"CREATE TABLE "glyph" ( "id" integer, "aspect" integer, "image" text,"#,
                &format!(r#"PRIMARY KEY ("id") {text},"#),
                &format!(
                    r#"CONSTRAINT "glyph_aspect" UNIQUE NULLS NOT DISTINCT ("aspect") INCLUDE ("image") {text} )"#
                ),
            ]
            .join(" ")
        );

        // A table constraint folds the clause into its own node.
        let constraints = parsed_nodes(&sql, "Constraint");
        assert_eq!(constraints.len(), 2, "{sql}");
        let deferrable = deferrability != Deferrability::NotDeferrable;
        let deferred = deferrability == Deferrability::DeferrableInitiallyDeferred;
        for constraint in &constraints {
            assert_eq!(constraint["deferrable"], deferrable, "{sql}");
            assert_eq!(constraint["initdeferred"], deferred, "{sql}");
        }
    }

    // A plain statement embeds as a constraint that says nothing about deferral.
    assert_eq!(
        Table::create(Glyph::Table)
            .col(ColumnDef::new(Glyph::Id).integer())
            .primary_key(Index::create(Glyph::Table, Glyph::Id))
            .to_string(),
        r#"CREATE TABLE "glyph" ( "id" integer, PRIMARY KEY ("id") )"#
    );
}

// [spec:pgorm:req:sql.ddl.deferrability/test]    `ALTER TABLE` spells a column's key as
// `ADD UNIQUE (…)` / `ADD PRIMARY KEY (…)`, the clause after it
// [spec:pgorm:req:sql.ddl.alter-table+5/test]
#[test]
fn an_added_key_carries_its_deferrability() {
    let sql = Table::alter(Glyph::Table)
        .modify_column(
            ColumnDef::new(Glyph::Aspect)
                .unique_key_deferrability(Deferrability::DeferrableInitiallyDeferred),
        )
        .modify_column(
            ColumnDef::new(Glyph::Id)
                .primary_key_deferrability(Deferrability::DeferrableInitiallyImmediate),
        )
        .to_string();
    assert_eq!(
        sql,
        [
            r#"ALTER TABLE "glyph""#,
            r#"ADD UNIQUE ("aspect") DEFERRABLE INITIALLY DEFERRED,"#,
            r#"ADD PRIMARY KEY ("id") DEFERRABLE INITIALLY IMMEDIATE"#,
        ]
        .join(" ")
    );
    let constraints = parsed_nodes(&sql, "Constraint");
    assert_eq!(constraints.len(), 2, "{sql}");
    assert_eq!(constraints[0]["initdeferred"], true);
    assert_eq!(constraints[1]["deferrable"], true);
    assert_eq!(constraints[1]["initdeferred"], false);
}
