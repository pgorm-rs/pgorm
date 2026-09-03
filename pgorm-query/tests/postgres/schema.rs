use super::*;
use crate::oracle::{assert_eq, assert_query_eq};
use pgorm_query::extension::{Extension, Type};

// [spec:pgorm:req:sql.ddl+5/test]    the whole DDL surface is reachable through the six
// entry-point helpers
#[test]
fn every_ddl_entry_point_is_reachable() {
    assert!(
        Table::create(Glyph::Table)
            .col(ColumnDef::new(Glyph::Id).integer())
            .to_string()
            .starts_with("CREATE TABLE")
    );
    assert!(
        Table::alter(Glyph::Table)
            .drop_column(Glyph::Id)
            .to_string()
            .starts_with("ALTER TABLE")
    );
    assert!(
        Table::drop(Glyph::Table)
            .to_string()
            .starts_with("DROP TABLE")
    );
    assert!(
        Table::rename(Glyph::Table, Alias::new("g"))
            .to_string()
            .starts_with("ALTER TABLE")
    );
    assert!(
        Table::truncate(Glyph::Table)
            .to_string()
            .starts_with("TRUNCATE TABLE")
    );

    assert!(
        Index::create(Glyph::Table, Glyph::Id)
            .name("idx")
            .to_string()
            .starts_with("CREATE INDEX")
    );
    assert!(Index::drop("idx").to_string().starts_with("DROP INDEX"));

    assert!(
        ForeignKey::create()
            .name("fk")
            .from(Char::Table, Char::FontId)
            .to(Font::Table, Font::Id)
            .to_string()
            .starts_with("ALTER TABLE")
    );
    assert!(
        ForeignKey::drop(Char::Table, "fk")
            .to_string()
            .starts_with("ALTER TABLE")
    );

    assert!(
        Type::create()
            .as_enum(Alias::new("tea"))
            .values([Alias::new("green")])
            .to_string()
            .starts_with("CREATE TYPE")
    );
    assert!(
        Type::alter()
            .name(Alias::new("tea"))
            .add_value(Alias::new("black"))
            .to_string()
            .starts_with("ALTER TYPE")
    );
    assert!(
        Type::drop()
            .name(Alias::new("tea"))
            .to_string()
            .starts_with("DROP TYPE")
    );

    assert!(
        Extension::create()
            .name("ltree")
            .to_string()
            .starts_with("CREATE EXTENSION")
    );
    assert!(
        Extension::drop()
            .name("ltree")
            .to_string()
            .starts_with("DROP EXTENSION")
    );

    assert!(
        Comment::on_table(Glyph::Table, "glyphs")
            .to_string()
            .starts_with("COMMENT ON TABLE")
    );
    assert!(
        Comment::on_column(Glyph::Table, Glyph::Id, "the id")
            .to_string()
            .starts_with("COMMENT ON COLUMN")
    );
}

// [spec:pgorm:req:sql.ddl+5/test]    table, index, foreign-key and comment statements render
// through their `Display`, which delegates to the corresponding `prepare_*` method on the single
// Postgres `QueryBuilder`
#[test]
fn ddl_statements_render_through_display() {
    fn assert_renders<S: std::fmt::Display>(statement: &S, expected: &str) {
        assert_query_eq(&statement.to_string(), expected);
    }

    assert_renders(
        Table::create(Glyph::Table).col(ColumnDef::new(Glyph::Id).integer()),
        r#"CREATE TABLE "glyph" ( "id" integer )"#,
    );
    assert_renders(
        &Table::alter(Glyph::Table).drop_column(Glyph::Id),
        r#"ALTER TABLE "glyph" DROP COLUMN "id""#,
    );
    assert_renders(&Table::drop(Glyph::Table), r#"DROP TABLE "glyph""#);
    assert_renders(
        &Table::rename(Glyph::Table, Alias::new("g")),
        r#"ALTER TABLE "glyph" RENAME TO "g""#,
    );
    assert_renders(&Table::truncate(Glyph::Table), r#"TRUNCATE TABLE "glyph""#);
    assert_renders(
        Index::create(Glyph::Table, Glyph::Id).name("idx"),
        r#"CREATE INDEX "idx" ON "glyph" ("id")"#,
    );
    assert_renders(&Index::drop("idx"), r#"DROP INDEX "idx""#);
    assert_renders(
        ForeignKey::create()
            .name("fk")
            .from(Char::Table, Char::FontId)
            .to(Font::Table, Font::Id),
        r#"ALTER TABLE "character" ADD CONSTRAINT "fk" FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#,
    );
    assert_renders(
        &ForeignKey::drop(Char::Table, "fk"),
        r#"ALTER TABLE "character" DROP CONSTRAINT "fk""#,
    );
    assert_renders(
        &Comment::on_table(Glyph::Table, "glyphs"),
        r#"COMMENT ON TABLE "glyph" IS 'glyphs'"#,
    );
}

// [spec:pgorm:req:sql.ddl+5/test]    `TableStatement` is an enum wrapper dispatching to the same
// builders
#[test]
fn table_statement_wrapper_dispatches() {
    let statements = [
        TableStatement::Create(
            Table::create(Glyph::Table)
                .col(ColumnDef::new(Glyph::Id).integer())
                .to_owned(),
        ),
        TableStatement::Alter(Table::alter(Glyph::Table).drop_column(Glyph::Id).to_owned()),
        TableStatement::Drop(Table::drop(Glyph::Table)),
        TableStatement::Rename(Table::rename(Glyph::Table, Alias::new("g"))),
        TableStatement::Truncate(Table::truncate(Glyph::Table)),
    ];

    let expected = [
        r#"CREATE TABLE "glyph" ( "id" integer )"#,
        r#"ALTER TABLE "glyph" DROP COLUMN "id""#,
        r#"DROP TABLE "glyph""#,
        r#"ALTER TABLE "glyph" RENAME TO "g""#,
        r#"TRUNCATE TABLE "glyph""#,
    ];

    for (statement, expected) in statements.iter().zip(expected) {
        assert_eq!(statement.to_string(), expected);
    }

    // `IndexStatement`, `ForeignKeyStatement` and `SchemaStatement` wrap the same
    // builders; the wrapped statement renders exactly as it does on its own.
    let schema_statements = [
        SchemaStatement::TableStatement(TableStatement::Drop(Table::drop(Glyph::Table))),
        SchemaStatement::IndexStatement(IndexStatement::Drop(Index::drop("idx"))),
        SchemaStatement::ForeignKeyStatement(ForeignKeyStatement::Drop(ForeignKey::drop(
            Char::Table,
            "fk",
        ))),
    ];

    let rendered: Vec<String> = schema_statements
        .iter()
        .map(|statement| match statement {
            SchemaStatement::TableStatement(inner) => inner.to_string(),
            SchemaStatement::IndexStatement(IndexStatement::Create(inner)) => inner.to_string(),
            SchemaStatement::IndexStatement(IndexStatement::Drop(inner)) => inner.to_string(),
            SchemaStatement::ForeignKeyStatement(ForeignKeyStatement::Create(inner)) => {
                inner.to_string()
            }
            SchemaStatement::ForeignKeyStatement(ForeignKeyStatement::Drop(inner)) => {
                inner.to_string()
            }
        })
        .collect();

    assert_eq!(
        rendered,
        vec![
            r#"DROP TABLE "glyph""#.to_owned(),
            r#"DROP INDEX "idx""#.to_owned(),
            r#"ALTER TABLE "character" DROP CONSTRAINT "fk""#.to_owned(),
        ]
    );
}

// [spec:pgorm:req:sql.ddl+5/test]    identifiers render double-quoted, with embedded quotes doubled
#[test]
fn ddl_identifiers_are_double_quoted() {
    assert_eq!(
        Table::create(Alias::new(r#"he"llo"#))
            .col(ColumnDef::new(Alias::new(r#"wor"ld"#)).integer())
            .to_string(),
        r#"CREATE TABLE "he""llo" ( "wor""ld" integer )"#
    );
    assert_eq!(
        Index::create((Alias::new("schema"), Glyph::Table), Glyph::Id)
            .name("idx")
            .to_string(),
        r#"CREATE INDEX "idx" ON "schema"."glyph" ("id")"#
    );
    assert_eq!(
        ForeignKey::create()
            .name("fk")
            .from(Char::Table, Char::FontId)
            .to(Font::Table, Font::Id)
            .to_string(),
        r#"ALTER TABLE "character" ADD CONSTRAINT "fk" FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#
    );
}

// [spec:pgorm:req:sql.ddl+5/test]    index and constraint names are idens, so an embedded
// quote is doubled at every site that writes one
#[test]
fn ddl_index_and_constraint_names_escape_quotes() {
    assert_eq!(
        Table::create(Glyph::Table)
            .col(ColumnDef::new(Glyph::Id).integer())
            .index(
                Index::create(Glyph::Table, Glyph::Id)
                    .name(r#"i"dx"#)
                    .unique()
            )
            .to_string(),
        r#"CREATE TABLE "glyph" ( "id" integer, CONSTRAINT "i""dx" UNIQUE ("id") )"#
    );
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Id)
            .name(r#"i"dx"#)
            .to_string(),
        r#"CREATE INDEX "i""dx" ON "glyph" ("id")"#
    );
    assert_eq!(Index::drop(r#"i"dx"#).to_string(), r#"DROP INDEX "i""dx""#);
    assert_eq!(
        ForeignKey::create()
            .name(r#"f"k"#)
            .from(Char::Table, Char::FontId)
            .to(Font::Table, Font::Id)
            .to_string(),
        r#"ALTER TABLE "character" ADD CONSTRAINT "f""k" FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#
    );
    assert_eq!(
        ForeignKey::drop(Char::Table, r#"f"k"#).to_string(),
        r#"ALTER TABLE "character" DROP CONSTRAINT "f""k""#
    );
    assert_eq!(
        Table::alter(Char::Table)
            .drop_foreign_key(Alias::new(r#"f"k"#))
            .to_string(),
        r#"ALTER TABLE "character" DROP CONSTRAINT "f""k""#
    );
}
