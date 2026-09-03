use super::*;
use crate::oracle::assert_eq;

// [spec:pgorm:req:sql.ddl.comment+1/test]    both targets render, at every level of qualification
#[test]
fn comment_statements_render_their_targets() {
    assert_eq!(
        Comment::on_table(Glyph::Table, "one row per glyph").to_string(QueryBuilder),
        r#"COMMENT ON TABLE "glyph" IS 'one row per glyph'"#
    );
    assert_eq!(
        Comment::on_table((Alias::new("public"), Glyph::Table), "qualified")
            .to_string(QueryBuilder),
        r#"COMMENT ON TABLE "public"."glyph" IS 'qualified'"#
    );
    assert_eq!(
        Comment::on_column(Glyph::Table, Glyph::Aspect, "the ratio").to_string(QueryBuilder),
        r#"COMMENT ON COLUMN "glyph"."aspect" IS 'the ratio'"#
    );
    assert_eq!(
        Comment::on_column(
            (Alias::new("public"), Glyph::Table),
            Glyph::Aspect,
            "the ratio"
        )
        .to_string(QueryBuilder),
        r#"COMMENT ON COLUMN "public"."glyph"."aspect" IS 'the ratio'"#
    );

    // The target and the unescaped text are readable back off the statement.
    let statement = Comment::on_column(Glyph::Table, Glyph::Aspect, "it's fine");
    assert_eq!(statement.get_comment(), "it's fine");
    match statement.get_target() {
        CommentTarget::Column(TableName::Table(table), column) => {
            assert_eq!(table.to_string(), "glyph");
            assert_eq!(column.to_string(), "aspect");
        }
        other => panic!("expected a bare-table column target, got {other:?}"),
    }
}

// [spec:pgorm:req:sql.ddl.comment+1/test]    comment text is a standard-conforming string literal:
// only the single quote is escaped, by doubling
#[test]
fn comment_text_is_a_quoted_literal() {
    assert_eq!(
        Comment::on_table(Glyph::Table, "it's a 'quoted' word").to_string(QueryBuilder),
        r#"COMMENT ON TABLE "glyph" IS 'it''s a ''quoted'' word'"#
    );

    // Backslashes are literal in a standard-conforming string, so they pass through.
    assert_eq!(
        Comment::on_table(Glyph::Table, r"C:\glyphs\ or \n").to_string(QueryBuilder),
        r#"COMMENT ON TABLE "glyph" IS 'C:\glyphs\ or \n'"#
    );

    // A statement-terminating attempt stays inside the literal.
    assert_eq!(
        Comment::on_table(Glyph::Table, "'; DROP TABLE glyph; --").to_string(QueryBuilder),
        r#"COMMENT ON TABLE "glyph" IS '''; DROP TABLE glyph; --'"#
    );

    assert_eq!(
        Comment::on_table(Glyph::Table, "").to_string(QueryBuilder),
        r#"COMMENT ON TABLE "glyph" IS ''"#
    );

    // Identifiers keep their own quoting rule: embedded double quotes are doubled.
    assert_eq!(
        Comment::on_column(Alias::new(r#"gl"yph"#), Alias::new(r#"as"pect"#), "odd")
            .to_string(QueryBuilder),
        r#"COMMENT ON COLUMN "gl""yph"."as""pect" IS 'odd'"#
    );
}

// [spec:pgorm:req:sql.ddl.comment+1/test]    one `TableName` value serves a comment target and a
// DDL target, so a comment cannot name a table the DDL beside it could not
#[test]
fn comment_and_ddl_share_one_table_name() {
    let name = (Alias::new("public"), Glyph::Table).into_table_name();

    assert_eq!(
        Comment::on_table(name.clone(), "shared").to_string(QueryBuilder),
        r#"COMMENT ON TABLE "public"."glyph" IS 'shared'"#
    );
    assert_eq!(
        Table::truncate(name.clone()).to_string(QueryBuilder),
        r#"TRUNCATE TABLE "public"."glyph""#
    );
    assert_eq!(
        name.schema().map(|s| s.to_string()).as_deref(),
        Some("public")
    );
    assert_eq!(name.table().to_string(), "glyph");
}
