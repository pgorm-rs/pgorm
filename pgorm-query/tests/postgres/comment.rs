use super::*;
use pretty_assertions::assert_eq;

// [spec:pgorm:req:sql.ddl.comment/test]    both targets render, at every level of qualification
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
        Comment::on_table(
            (Alias::new("db"), Alias::new("public"), Glyph::Table),
            "fully qualified"
        )
        .to_string(QueryBuilder),
        r#"COMMENT ON TABLE "db"."public"."glyph" IS 'fully qualified'"#
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
        CommentTarget::Column(CommentTable::Table(table), column) => {
            assert_eq!(table.to_string(), "glyph");
            assert_eq!(column.to_string(), "aspect");
        }
        other => panic!("expected a bare-table column target, got {other:?}"),
    }
}

// [spec:pgorm:req:sql.ddl.comment/test]    comment text is a standard-conforming string literal:
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

// [spec:pgorm:req:sql.ddl.comment/test]    a TableRef converts by dropping its alias, and the
// forms that name no table are rejected rather than panicking
#[test]
fn comment_table_from_table_ref() {
    assert_eq!(
        CommentTable::try_from(Glyph::Table.into_table_ref()),
        Ok(CommentTable::Table(Glyph::Table.into_iden()))
    );
    assert_eq!(
        CommentTable::try_from(Glyph::Table.into_table_ref().alias(Alias::new("g"))),
        Ok(CommentTable::Table(Glyph::Table.into_iden())),
        "an alias is a query-scope name; the comment lands on the table"
    );

    let qualified = (Alias::new("public"), Glyph::Table).into_table_ref();
    assert_eq!(
        CommentTable::try_from(qualified.clone().alias(Alias::new("g"))),
        CommentTable::try_from(qualified)
    );
    assert_eq!(
        CommentTable::try_from(
            (Alias::new("db"), Alias::new("public"), Glyph::Table).into_table_ref()
        ),
        Ok(CommentTable::DatabaseSchemaTable(
            Alias::new("db").into_iden(),
            Alias::new("public").into_iden(),
            Glyph::Table.into_iden()
        ))
    );

    for unnamed in [
        TableRef::SubQuery(
            Query::select().column(Glyph::Id).from(Glyph::Table).take(),
            Alias::new("q").into_iden(),
        ),
        TableRef::ValuesList(vec![], Alias::new("v").into_iden()),
    ] {
        assert_eq!(CommentTable::try_from(unnamed), Err(UnnamedTableRef));
    }
    assert_eq!(
        UnnamedTableRef.to_string(),
        "table reference does not name a table"
    );
}
