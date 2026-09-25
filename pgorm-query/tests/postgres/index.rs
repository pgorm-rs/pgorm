use super::*;
use crate::oracle::assert_eq;

// [spec:pgorm:req:sql.ddl.index-create+9/test]
#[test]
fn create_1() {
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Aspect)
            .name(Name::runtime("idx-glyph-aspect"))
            .to_string(),
        r#"CREATE INDEX "idx-glyph-aspect" ON "glyph" ("aspect")"#
    );
}

#[test]
fn create_2() {
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Aspect)
            .unique()
            .name(Name::runtime("idx-glyph-aspect-image"))
            .col(Glyph::Image)
            .to_string(),
        r#"CREATE UNIQUE INDEX "idx-glyph-aspect-image" ON "glyph" ("aspect", "image")"#
    );
}

#[test]
fn create_3() {
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Image)
            .gin()
            .name(Name::runtime("idx-glyph-image"))
            .to_string(),
        r#"CREATE INDEX "idx-glyph-image" ON "glyph" USING GIN ("image")"#
    );
}

#[test]
fn create_4() {
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Image)
            .if_not_exists()
            .gin()
            .name(Name::runtime("idx-glyph-image"))
            .to_string(),
        r#"CREATE INDEX IF NOT EXISTS "idx-glyph-image" ON "glyph" USING GIN ("image")"#
    );
}

#[test]
fn create_5() {
    assert_eq!(
        Index::create((Name::runtime("schema"), Glyph::Table), Glyph::Aspect)
            .unique()
            .name(Name::runtime("idx-glyph-aspect-image"))
            .col(Glyph::Image)
            .to_string(),
        r#"CREATE UNIQUE INDEX "idx-glyph-aspect-image" ON "schema"."glyph" ("aspect", "image")"#
    );
}

// [spec:pgorm:req:sql.ddl.index-create+9/test]
#[test]
fn create_6() {
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Aspect)
            .unique()
            .nulls_not_distinct()
            .name(Name::runtime("idx-glyph-aspect-image"))
            .col(Glyph::Image)
            .to_string(),
        r#"CREATE UNIQUE INDEX "idx-glyph-aspect-image" ON "glyph" ("aspect", "image") NULLS NOT DISTINCT"#
    );
}

// [spec:pgorm:req:sql.ddl.index-create+9/test]
#[test]
fn standalone_index_spells_plain_or_unique_only() {
    let index = || {
        Index::create(Glyph::Table, Glyph::Aspect)
            .name(Name::runtime("idx"))
            .to_owned()
    };
    let plain = r#"CREATE INDEX "idx" ON "glyph" ("aspect")"#;
    let unique = r#"CREATE UNIQUE INDEX "idx" ON "glyph" ("aspect")"#;

    assert_eq!(index().to_string(), plain);
    assert_eq!(index().unique().to_string(), unique);
    assert_eq!(index().primary().to_string(), plain);
    assert_eq!(index().primary().unique().to_string(), unique);
    assert_eq!(index().unique().primary().to_string(), plain);
}

// [spec:pgorm:req:sql.ddl.index-create+9/test]
#[test]
fn index_kind_accessors_are_mutually_exclusive() {
    let index = Index::create(Glyph::Table, Glyph::Aspect);

    assert_eq!(index.kind(), IndexKind::Plain);
    assert!(!index.is_primary_key());
    assert!(!index.is_unique_key());

    let unique = index.clone().unique().to_owned();
    assert!(unique.is_unique_key());
    assert!(!unique.is_primary_key());

    let primary = index.clone().unique().primary().to_owned();
    assert!(primary.is_primary_key());
    assert!(!primary.is_unique_key());
}

// [spec:pgorm:req:sql.ddl.index-create+9/test]
#[test]
fn nulls_not_distinct_needs_the_unique_kind() {
    let index = || {
        Index::create(Glyph::Table, Glyph::Aspect)
            .name(Name::runtime("idx"))
            .nulls_not_distinct()
            .to_owned()
    };
    let plain = r#"CREATE INDEX "idx" ON "glyph" ("aspect")"#;

    assert_eq!(index().to_string(), plain);
    assert_eq!(index().primary().to_string(), plain);
    assert_eq!(
        index().unique().to_string(),
        r#"CREATE UNIQUE INDEX "idx" ON "glyph" ("aspect") NULLS NOT DISTINCT"#
    );
}

// [spec:pgorm:req:sql.ddl.index-drop+3/test]
#[test]
fn drop_1() {
    assert_eq!(
        Index::drop(Name::runtime("idx-glyph-aspect")).to_string(),
        r#"DROP INDEX "idx-glyph-aspect""#
    );
}

// [spec:pgorm:req:sql.ddl.index-drop+3/test]
#[test]
fn drop_2() {
    assert_eq!(
        Index::drop(Name::runtime("idx-glyph-aspect"))
            .table((Name::runtime("schema"), Glyph::Table))
            .to_string(),
        r#"DROP INDEX "schema"."idx-glyph-aspect""#
    );
}

#[test]
fn drop_3() {
    assert_eq!(
        Index::drop(Name::runtime("idx-glyph-aspect"))
            .table(Glyph::Table)
            .to_string(),
        r#"DROP INDEX "idx-glyph-aspect""#
    );
}

// [spec:pgorm:req:sql.ddl.index-create+9/test]    an index embeds by value: the source survives
// only because the call site cloned it, and a chained builder has to say `.to_owned()`
#[test]
fn index_embeds_by_value() {
    let index = Index::create(Glyph::Table, Glyph::Aspect)
        .name(Name::runtime("idx"))
        .unique()
        .to_owned();

    assert_eq!(
        Table::create(Glyph::Table)
            .col(ColumnDef::new(Glyph::Aspect).integer())
            .index(index.clone())
            .to_string(),
        [
            r#"CREATE TABLE "glyph" ("#,
            r#""aspect" integer,"#,
            r#"CONSTRAINT "idx" UNIQUE ("aspect")"#,
            r#")"#,
        ]
        .join(" ")
    );
    assert_eq!(
        index.to_string(),
        r#"CREATE UNIQUE INDEX "idx" ON "glyph" ("aspect")"#
    );
}

// [spec:pgorm:req:sql.ddl.index-create+9/test]    a predicate closes the
// statement, and repeated calls conjoin as they do everywhere else
#[test]
fn partial_index_carries_a_predicate() {
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Aspect)
            .name(Name::runtime("idx-glyph-aspect-live"))
            .unique()
            .and_where(Expr::col(Glyph::Image).is_not_null())
            .to_string(),
        [
            r#"CREATE UNIQUE INDEX "idx-glyph-aspect-live" ON "glyph" ("aspect")"#,
            r#"WHERE "image" IS NOT NULL"#,
        ]
        .join(" ")
    );

    assert_eq!(
        Index::create(Glyph::Table, Glyph::Aspect)
            .name(Name::runtime("idx"))
            .and_where(Expr::col(Glyph::Aspect).gt(3))
            .and_where(Expr::col(Glyph::Image).is_not_null())
            .to_string(),
        [
            r#"CREATE INDEX "idx" ON "glyph" ("aspect")"#,
            r#"WHERE "aspect" > 3 AND "image" IS NOT NULL"#,
        ]
        .join(" ")
    );

    // No predicate, no keyword.
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Aspect)
            .name(Name::runtime("idx"))
            .to_string(),
        r#"CREATE INDEX "idx" ON "glyph" ("aspect")"#
    );
}

// [spec:pgorm:req:sql.ddl.index-create+9/test]    an expression entry is
// parenthesised where a column name would stand bare, and composes with order
#[test]
fn expression_index_parenthesises_its_expression() {
    assert_eq!(
        Index::create(
            Glyph::Table,
            IndexColumn::expr(Func::lower(Expr::col(Glyph::Image)))
        )
        .name(Name::runtime("idx-glyph-image-lower"))
        .to_string(),
        r#"CREATE INDEX "idx-glyph-image-lower" ON "glyph" ((LOWER("image")))"#
    );

    assert_eq!(
        Index::create(
            Glyph::Table,
            IndexColumn::expr(Func::lower(Expr::col(Glyph::Image))).order(IndexOrder::Desc)
        )
        .name(Name::runtime("idx"))
        .col((Glyph::Aspect, IndexOrder::Asc))
        .to_string(),
        r#"CREATE INDEX "idx" ON "glyph" ((LOWER("image")) DESC, "aspect" ASC)"#
    );
}

// [spec:pgorm:req:sql.ddl.index-create+9/test]    an operator class sits between
// the entry and its order, on a named entry and an expression entry alike
#[test]
fn operator_class_precedes_the_order() {
    assert_eq!(
        Index::create(
            Glyph::Table,
            IndexColumn::name(Glyph::Image).operator_class(Name::runtime("text_pattern_ops"))
        )
        .name(Name::runtime("idx"))
        .to_string(),
        r#"CREATE INDEX "idx" ON "glyph" ("image" "text_pattern_ops")"#
    );

    assert_eq!(
        Index::create(
            Glyph::Table,
            IndexColumn::expr(Func::lower(Expr::col(Glyph::Image)))
                .operator_class(Name::runtime("text_pattern_ops"))
                .order(IndexOrder::Desc)
        )
        .name(Name::runtime("idx"))
        .to_string(),
        r#"CREATE INDEX "idx" ON "glyph" ((LOWER("image")) "text_pattern_ops" DESC)"#
    );
}

// [spec:pgorm:req:sql.ddl.index-create+9/test]    INCLUDE follows the key list
// in both the standalone and the embedded position
#[test]
fn include_carries_non_key_columns() {
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Aspect)
            .name(Name::runtime("idx"))
            .unique()
            .include([Glyph::Image, Glyph::Tokens])
            .nulls_not_distinct()
            .to_string(),
        [
            r#"CREATE UNIQUE INDEX "idx" ON "glyph" ("aspect")"#,
            r#"INCLUDE ("image", "tokens") NULLS NOT DISTINCT"#,
        ]
        .join(" ")
    );

    assert_eq!(
        Table::create(Glyph::Table)
            .col(ColumnDef::new(Glyph::Aspect).integer())
            .col(ColumnDef::new(Glyph::Image).text())
            .index(
                Index::create(Glyph::Table, Glyph::Aspect)
                    .name(Name::runtime("idx"))
                    .unique()
                    .include([Glyph::Image])
                    .to_owned()
            )
            .to_string(),
        [
            r#"CREATE TABLE "glyph" ("#,
            r#""aspect" integer,"#,
            r#""image" text,"#,
            r#"CONSTRAINT "idx" UNIQUE ("aspect") INCLUDE ("image")"#,
            r#")"#,
        ]
        .join(" ")
    );

    // Repeated calls append rather than replace.
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Aspect)
            .name(Name::runtime("idx"))
            .include([Glyph::Image])
            .include([Glyph::Tokens])
            .to_string(),
        r#"CREATE INDEX "idx" ON "glyph" ("aspect") INCLUDE ("image", "tokens")"#
    );
}

// [spec:pgorm:req:sql.ddl.index-create+9/test]    every clause at once, in the
// order PostgreSQL's grammar puts them
#[test]
fn every_index_clause_composes() {
    assert_eq!(
        Index::create(
            Glyph::Table,
            IndexColumn::expr(Func::lower(Expr::col(Glyph::Image))).order(IndexOrder::Desc)
        )
        .name(Name::runtime("idx-everything"))
        .if_not_exists()
        .unique()
        .col((Glyph::Aspect, IndexOrder::Asc))
        .include([Glyph::Tokens])
        .nulls_not_distinct()
        .and_where(Expr::col(Glyph::Aspect).gt(0))
        .to_string(),
        [
            r#"CREATE UNIQUE INDEX IF NOT EXISTS "idx-everything" ON "glyph""#,
            r#"((LOWER("image")) DESC, "aspect" ASC) INCLUDE ("tokens")"#,
            r#"NULLS NOT DISTINCT WHERE "aspect" > 0"#,
        ]
        .join(" ")
    );
}
