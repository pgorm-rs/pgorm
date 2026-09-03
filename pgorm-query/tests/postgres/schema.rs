use super::*;
use crate::oracle::{assert_eq, assert_query_eq};
use pgorm_query::extension::{Extension, Type};

// [spec:pgorm:req:sql.ddl+4/test]    the whole DDL surface is reachable through the six
// entry-point helpers
#[test]
fn every_ddl_entry_point_is_reachable() {
    assert!(
        Table::create()
            .table(Glyph::Table)
            .col(ColumnDef::new(Glyph::Id).integer())
            .to_string(QueryBuilder)
            .starts_with("CREATE TABLE")
    );
    assert!(
        Table::alter(Glyph::Table)
            .drop_column(Glyph::Id)
            .to_string(QueryBuilder)
            .starts_with("ALTER TABLE")
    );
    assert!(
        Table::drop()
            .table(Glyph::Table)
            .to_string(QueryBuilder)
            .starts_with("DROP TABLE")
    );
    assert!(
        Table::rename()
            .table(Glyph::Table, Alias::new("g"))
            .to_string(QueryBuilder)
            .starts_with("ALTER TABLE")
    );
    assert!(
        Table::truncate()
            .table(Glyph::Table)
            .to_string(QueryBuilder)
            .starts_with("TRUNCATE TABLE")
    );

    assert!(
        Index::create(Glyph::Id)
            .name("idx")
            .table(Glyph::Table)
            .to_string(QueryBuilder)
            .starts_with("CREATE INDEX")
    );
    assert!(
        Index::drop()
            .name("idx")
            .to_string(QueryBuilder)
            .starts_with("DROP INDEX")
    );

    assert!(
        ForeignKey::create()
            .name("fk")
            .from(Char::Table, Char::FontId)
            .to(Font::Table, Font::Id)
            .to_string(QueryBuilder)
            .starts_with("ALTER TABLE")
    );
    assert!(
        ForeignKey::drop()
            .name("fk")
            .table(Char::Table)
            .to_string(QueryBuilder)
            .starts_with("ALTER TABLE")
    );

    assert!(
        Type::create()
            .as_enum(Alias::new("tea"))
            .values([Alias::new("green")])
            .to_string(QueryBuilder)
            .starts_with("CREATE TYPE")
    );
    assert!(
        Type::alter()
            .name(Alias::new("tea"))
            .add_value(Alias::new("black"))
            .to_string(QueryBuilder)
            .starts_with("ALTER TYPE")
    );
    assert!(
        Type::drop()
            .name(Alias::new("tea"))
            .to_string(QueryBuilder)
            .starts_with("DROP TYPE")
    );

    assert!(
        Extension::create()
            .name("ltree")
            .to_string(QueryBuilder)
            .starts_with("CREATE EXTENSION")
    );
    assert!(
        Extension::drop()
            .name("ltree")
            .to_string(QueryBuilder)
            .starts_with("DROP EXTENSION")
    );

    assert!(
        Comment::on_table(Glyph::Table, "glyphs")
            .to_string(QueryBuilder)
            .starts_with("COMMENT ON TABLE")
    );
    assert!(
        Comment::on_column(Glyph::Table, Glyph::Id, "the id")
            .to_string(QueryBuilder)
            .starts_with("COMMENT ON COLUMN")
    );
}

// [spec:pgorm:req:sql.ddl+4/test]    table, index, foreign-key and comment statements implement
// `SchemaStatementBuilder`, whose `build` / `build_any` / `to_string` all delegate to the same
// `prepare_*` method on the single Postgres `QueryBuilder`
#[test]
fn schema_statement_builder_trio_agrees() {
    fn assert_trio<S: SchemaStatementBuilder>(statement: &S, expected: &str) {
        assert_query_eq(&statement.build(QueryBuilder), expected);
        assert_query_eq(&statement.build_any(&QueryBuilder), expected);
        assert_query_eq(
            &SchemaStatementBuilder::to_string(statement, QueryBuilder),
            expected,
        );
    }

    assert_trio(
        Table::create()
            .table(Glyph::Table)
            .col(ColumnDef::new(Glyph::Id).integer()),
        r#"CREATE TABLE "glyph" ( "id" integer )"#,
    );
    assert_trio(
        &Table::alter(Glyph::Table).drop_column(Glyph::Id),
        r#"ALTER TABLE "glyph" DROP COLUMN "id""#,
    );
    assert_trio(Table::drop().table(Glyph::Table), r#"DROP TABLE "glyph""#);
    assert_trio(
        Table::rename().table(Glyph::Table, Alias::new("g")),
        r#"ALTER TABLE "glyph" RENAME TO "g""#,
    );
    assert_trio(
        Table::truncate().table(Glyph::Table),
        r#"TRUNCATE TABLE "glyph""#,
    );
    assert_trio(
        Index::create(Glyph::Id).name("idx").table(Glyph::Table),
        r#"CREATE INDEX "idx" ON "glyph" ("id")"#,
    );
    assert_trio(Index::drop().name("idx"), r#"DROP INDEX "idx""#);
    assert_trio(
        ForeignKey::create()
            .name("fk")
            .from(Char::Table, Char::FontId)
            .to(Font::Table, Font::Id),
        r#"ALTER TABLE "character" ADD CONSTRAINT "fk" FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#,
    );
    assert_trio(
        ForeignKey::drop().name("fk").table(Char::Table),
        r#"ALTER TABLE "character" DROP CONSTRAINT "fk""#,
    );
    assert_trio(
        &Comment::on_table(Glyph::Table, "glyphs"),
        r#"COMMENT ON TABLE "glyph" IS 'glyphs'"#,
    );
}

// [spec:pgorm:req:sql.ddl+4/test]    `TableStatement` is an enum wrapper dispatching to the same
// builders
#[test]
fn table_statement_wrapper_dispatches() {
    let statements = [
        TableStatement::Create(
            Table::create()
                .table(Glyph::Table)
                .col(ColumnDef::new(Glyph::Id).integer())
                .to_owned(),
        ),
        TableStatement::Alter(Table::alter(Glyph::Table).drop_column(Glyph::Id).to_owned()),
        TableStatement::Drop(Table::drop().table(Glyph::Table).to_owned()),
        TableStatement::Rename(
            Table::rename()
                .table(Glyph::Table, Alias::new("g"))
                .to_owned(),
        ),
        TableStatement::Truncate(Table::truncate().table(Glyph::Table).to_owned()),
    ];

    let expected = [
        r#"CREATE TABLE "glyph" ( "id" integer )"#,
        r#"ALTER TABLE "glyph" DROP COLUMN "id""#,
        r#"DROP TABLE "glyph""#,
        r#"ALTER TABLE "glyph" RENAME TO "g""#,
        r#"TRUNCATE TABLE "glyph""#,
    ];

    for (statement, expected) in statements.iter().zip(expected) {
        assert_eq!(statement.build(QueryBuilder), expected);
        assert_eq!(statement.build_any(&QueryBuilder), expected);
        assert_eq!(statement.to_string(QueryBuilder), expected);
    }

    // `IndexStatement`, `ForeignKeyStatement` and `SchemaStatement` wrap the same
    // builders; the wrapped statement renders exactly as it does on its own.
    let schema_statements = [
        SchemaStatement::TableStatement(TableStatement::Drop(
            Table::drop().table(Glyph::Table).to_owned(),
        )),
        SchemaStatement::IndexStatement(IndexStatement::Drop(Index::drop().name("idx").to_owned())),
        SchemaStatement::ForeignKeyStatement(ForeignKeyStatement::Drop(
            ForeignKey::drop().name("fk").table(Char::Table).to_owned(),
        )),
    ];

    let rendered: Vec<String> = schema_statements
        .iter()
        .map(|statement| match statement {
            SchemaStatement::TableStatement(inner) => inner.to_string(QueryBuilder),
            SchemaStatement::IndexStatement(IndexStatement::Create(inner)) => {
                inner.to_string(QueryBuilder)
            }
            SchemaStatement::IndexStatement(IndexStatement::Drop(inner)) => {
                inner.to_string(QueryBuilder)
            }
            SchemaStatement::ForeignKeyStatement(ForeignKeyStatement::Create(inner)) => {
                inner.to_string(QueryBuilder)
            }
            SchemaStatement::ForeignKeyStatement(ForeignKeyStatement::Drop(inner)) => {
                inner.to_string(QueryBuilder)
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

// [spec:pgorm:req:sql.ddl+4/test]    identifiers render double-quoted, with embedded quotes doubled
#[test]
fn ddl_identifiers_are_double_quoted() {
    assert_eq!(
        Table::create()
            .table(Alias::new(r#"he"llo"#))
            .col(ColumnDef::new(Alias::new(r#"wor"ld"#)).integer())
            .to_string(QueryBuilder),
        r#"CREATE TABLE "he""llo" ( "wor""ld" integer )"#
    );
    assert_eq!(
        Index::create(Glyph::Id)
            .name("idx")
            .table((Alias::new("schema"), Glyph::Table))
            .to_string(QueryBuilder),
        r#"CREATE INDEX "idx" ON "schema"."glyph" ("id")"#
    );
    assert_eq!(
        ForeignKey::create()
            .name("fk")
            .from(Char::Table, Char::FontId)
            .to(Font::Table, Font::Id)
            .to_string(QueryBuilder),
        r#"ALTER TABLE "character" ADD CONSTRAINT "fk" FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#
    );
}

// [spec:pgorm:req:sql.ddl+4/test]    index and constraint names are idens, so an embedded
// quote is doubled at every site that writes one
#[test]
fn ddl_index_and_constraint_names_escape_quotes() {
    assert_eq!(
        Table::create()
            .table(Glyph::Table)
            .col(ColumnDef::new(Glyph::Id).integer())
            .index(Index::create(Glyph::Id).name(r#"i"dx"#).unique())
            .to_string(QueryBuilder),
        r#"CREATE TABLE "glyph" ( "id" integer, CONSTRAINT "i""dx" UNIQUE ("id") )"#
    );
    assert_eq!(
        Index::create(Glyph::Id)
            .name(r#"i"dx"#)
            .table(Glyph::Table)
            .to_string(QueryBuilder),
        r#"CREATE INDEX "i""dx" ON "glyph" ("id")"#
    );
    assert_eq!(
        Index::drop().name(r#"i"dx"#).to_string(QueryBuilder),
        r#"DROP INDEX "i""dx""#
    );
    assert_eq!(
        ForeignKey::create()
            .name(r#"f"k"#)
            .from(Char::Table, Char::FontId)
            .to(Font::Table, Font::Id)
            .to_string(QueryBuilder),
        r#"ALTER TABLE "character" ADD CONSTRAINT "f""k" FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#
    );
    assert_eq!(
        ForeignKey::drop()
            .name(r#"f"k"#)
            .table(Char::Table)
            .to_string(QueryBuilder),
        r#"ALTER TABLE "character" DROP CONSTRAINT "f""k""#
    );
    assert_eq!(
        Table::alter(Char::Table)
            .drop_foreign_key(Alias::new(r#"f"k"#))
            .to_string(QueryBuilder),
        r#"ALTER TABLE "character" DROP CONSTRAINT "f""k""#
    );
}
