use super::*;
use crate::oracle::assert_eq;
use pgorm_query::extension::{Extension, PgLTree};

// [spec:pgorm:req:sql.ddl.extension+2/test]    every part of the CREATE EXTENSION grammar
#[test]
fn create_1() {
    assert_eq!(
        Extension::create().name("ltree").to_string(QueryBuilder),
        r#"CREATE EXTENSION "ltree""#
    );
}

// [spec:pgorm:req:sql.ddl.extension+2/test]
#[test]
fn create_2() {
    assert_eq!(
        Extension::create()
            .name("ltree")
            .schema("public")
            .version("v0.1.0")
            .cascade()
            .if_not_exists()
            .to_string(QueryBuilder),
        r#"CREATE EXTENSION IF NOT EXISTS "ltree" WITH SCHEMA "public" VERSION 'v0.1.0' CASCADE"#
    );
}

// [spec:pgorm:req:sql.ddl.extension+2/test]    `PgLTree` is a ready-made `Iden` rendering `ltree`,
// usable as an extension name through `From<PgLTree> for String`
#[test]
fn create_3() {
    assert_eq!(Iden::to_string(&PgLTree), "ltree");
    assert_eq!(
        Extension::create().name(PgLTree).to_string(QueryBuilder),
        r#"CREATE EXTENSION "ltree""#
    );

    // The matching column type is `ColumnType::LTree`.
    assert_eq!(
        Table::create(Glyph::Table)
            .col(ColumnDef::new(Glyph::Tokens).ltree())
            .to_string(QueryBuilder),
        r#"CREATE TABLE "glyph" ( "tokens" ltree )"#
    );
}

// [spec:pgorm:req:sql.ddl.extension+2/test]    on drop, `cascade` and `restrict` share one slot,
// so the last call wins and the pair PostgreSQL rejects cannot be built
#[test]
fn drop_1() {
    assert_eq!(
        Extension::drop().name("ltree").to_string(QueryBuilder),
        r#"DROP EXTENSION "ltree""#
    );
    assert_eq!(
        Extension::drop()
            .name("ltree")
            .if_exists()
            .cascade()
            .to_string(QueryBuilder),
        r#"DROP EXTENSION IF EXISTS "ltree" CASCADE"#
    );
    assert_eq!(
        Extension::drop()
            .name("ltree")
            .restrict()
            .to_string(QueryBuilder),
        r#"DROP EXTENSION "ltree" RESTRICT"#
    );
    assert_eq!(
        Extension::drop()
            .name("ltree")
            .restrict()
            .cascade()
            .to_string(QueryBuilder),
        r#"DROP EXTENSION "ltree" CASCADE"#
    );
}

// [spec:pgorm:sem:sql.render.ddl.extension+1/test]    name and schema are quoted identifiers and
// version a quoted literal, so an embedded quote escapes rather than ending the token
#[test]
fn extension_strings_are_quoted() {
    let mut sql = SqlWriterValues::new("$", true);
    let statement = Extension::create()
        .name(r#"pg"weird ext"#)
        .schema("my schema")
        .version("1.0; --")
        .to_owned();

    assert_eq!(
        statement.build_collect(QueryBuilder, &mut sql),
        r#"CREATE EXTENSION "pg""weird ext" WITH SCHEMA "my schema" VERSION '1.0; --'"#
    );
    // Nothing was parameterized on the way through a placeholder-emitting sink.
    let (_, values) = sql.into_parts();
    assert_eq!(values, Values(vec![]));

    assert_eq!(
        Extension::drop()
            .name(r#"pg"weird ext"#)
            .to_string(QueryBuilder),
        r#"DROP EXTENSION "pg""weird ext""#
    );
}
