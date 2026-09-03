use super::*;
use crate::oracle::assert_eq;

// [spec:pgorm:req:sql.ddl.index-create+4/test]
#[test]
fn create_1() {
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Aspect)
            .name("idx-glyph-aspect")
            .to_string(),
        r#"CREATE INDEX "idx-glyph-aspect" ON "glyph" ("aspect")"#
    );
}

#[test]
fn create_2() {
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Aspect)
            .unique()
            .name("idx-glyph-aspect-image")
            .col(Glyph::Image)
            .to_string(),
        r#"CREATE UNIQUE INDEX "idx-glyph-aspect-image" ON "glyph" ("aspect", "image")"#
    );
}

#[test]
fn create_3() {
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Image)
            .full_text()
            .name("idx-glyph-image")
            .to_string(),
        r#"CREATE INDEX "idx-glyph-image" ON "glyph" USING GIN ("image")"#
    );
}

#[test]
fn create_4() {
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Image)
            .if_not_exists()
            .full_text()
            .name("idx-glyph-image")
            .to_string(),
        r#"CREATE INDEX IF NOT EXISTS "idx-glyph-image" ON "glyph" USING GIN ("image")"#
    );
}

#[test]
fn create_5() {
    assert_eq!(
        Index::create((Alias::new("schema"), Glyph::Table), Glyph::Aspect)
            .unique()
            .name("idx-glyph-aspect-image")
            .col(Glyph::Image)
            .to_string(),
        r#"CREATE UNIQUE INDEX "idx-glyph-aspect-image" ON "schema"."glyph" ("aspect", "image")"#
    );
}

// [spec:pgorm:req:sql.ddl.index-create+4/test]
#[test]
fn create_6() {
    assert_eq!(
        Index::create(Glyph::Table, Glyph::Aspect)
            .unique()
            .nulls_not_distinct()
            .name("idx-glyph-aspect-image")
            .col(Glyph::Image)
            .to_string(),
        r#"CREATE UNIQUE INDEX "idx-glyph-aspect-image" ON "glyph" ("aspect", "image") NULLS NOT DISTINCT"#
    );
}

// [spec:pgorm:req:sql.ddl.index-create+4/test]
#[test]
fn standalone_index_spells_plain_or_unique_only() {
    let index = || {
        Index::create(Glyph::Table, Glyph::Aspect)
            .name("idx")
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

// [spec:pgorm:req:sql.ddl.index-create+4/test]
#[test]
fn index_kind_accessors_are_mutually_exclusive() {
    let index = Index::create(Glyph::Table, Glyph::Aspect).to_owned();

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

// [spec:pgorm:req:sql.ddl.index-create+4/test]
#[test]
fn nulls_not_distinct_needs_the_unique_kind() {
    let index = || {
        Index::create(Glyph::Table, Glyph::Aspect)
            .name("idx")
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

// [spec:pgorm:req:sql.ddl.index-drop+2/test]
#[test]
fn drop_1() {
    assert_eq!(
        Index::drop("idx-glyph-aspect").to_string(),
        r#"DROP INDEX "idx-glyph-aspect""#
    );
}

// [spec:pgorm:req:sql.ddl.index-drop+2/test]
#[test]
fn drop_2() {
    assert_eq!(
        Index::drop("idx-glyph-aspect")
            .table((Alias::new("schema"), Glyph::Table))
            .to_string(),
        r#"DROP INDEX "schema"."idx-glyph-aspect""#
    );
}

#[test]
fn drop_3() {
    assert_eq!(
        Index::drop("idx-glyph-aspect")
            .table(Glyph::Table)
            .to_string(),
        r#"DROP INDEX "idx-glyph-aspect""#
    );
}

// [spec:pgorm:req:sql.ddl.index-create+4/test]    take copies, so the source keeps its columns
#[test]
fn index_take_leaves_the_source_whole() {
    let mut index = Index::create(Glyph::Table, Glyph::Aspect)
        .name("idx")
        .to_owned();
    let taken = index.take();

    assert_eq!(
        taken.to_string(),
        r#"CREATE INDEX "idx" ON "glyph" ("aspect")"#
    );
    assert_eq!(
        index.to_string(),
        r#"CREATE INDEX "idx" ON "glyph" ("aspect")"#
    );
}
