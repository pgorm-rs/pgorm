use super::*;
use crate::oracle::assert_eq;

// [spec:pgorm:req:sql.ddl.foreign-key+2/test]
#[test]
fn create_1() {
    assert_eq!(
        ForeignKey::create()
            .name("FK_2e303c3a712662f1fc2a4d0aad6")
            .from(Char::Table, Char::FontId)
            .to(Font::Table, Font::Id)
            .on_delete(ForeignKeyAction::Cascade)
            .on_update(ForeignKeyAction::Cascade)
            .to_string(QueryBuilder),
        [
            r#"ALTER TABLE "character" ADD CONSTRAINT "FK_2e303c3a712662f1fc2a4d0aad6""#,
            r#"FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#,
            r#"ON DELETE CASCADE ON UPDATE CASCADE"#,
        ]
        .join(" ")
    );
}

#[test]
fn create_2() {
    assert_eq!(
        ForeignKey::create()
            .name("FK_2e303c3a712662f1fc2a4d0aad6")
            .from((Alias::new("schema"), Char::Table), Char::FontId)
            .to(Font::Table, Font::Id)
            .on_delete(ForeignKeyAction::Cascade)
            .on_update(ForeignKeyAction::Cascade)
            .to_string(QueryBuilder),
        [
            r#"ALTER TABLE "schema"."character" ADD CONSTRAINT "FK_2e303c3a712662f1fc2a4d0aad6""#,
            r#"FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#,
            r#"ON DELETE CASCADE ON UPDATE CASCADE"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:sql.ddl.foreign-key+2/test]
#[test]
fn drop_1() {
    assert_eq!(
        ForeignKey::drop(Char::Table, "FK_2e303c3a712662f1fc2a4d0aad6").to_string(QueryBuilder),
        r#"ALTER TABLE "character" DROP CONSTRAINT "FK_2e303c3a712662f1fc2a4d0aad6""#
    );
}

#[test]
fn drop_2() {
    assert_eq!(
        ForeignKey::drop(
            (Alias::new("schema"), Char::Table),
            "FK_2e303c3a712662f1fc2a4d0aad6"
        )
        .to_string(QueryBuilder),
        r#"ALTER TABLE "schema"."character" DROP CONSTRAINT "FK_2e303c3a712662f1fc2a4d0aad6""#
    );
}
