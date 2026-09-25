use super::*;
use crate::oracle::assert_eq;

// [spec:pgorm:req:sql.ddl.foreign-key+6/test]
#[test]
fn create_1() {
    assert_eq!(
        ForeignKey::create(Char::Table, Char::FontId, Font::Table, Font::Id)
            .name(Name::runtime("FK_2e303c3a712662f1fc2a4d0aad6"))
            .on_delete(ForeignKeyAction::Cascade)
            .on_update(ForeignKeyAction::Cascade)
            .to_string(),
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
        ForeignKey::create(
            (Name::runtime("schema"), Char::Table),
            Char::FontId,
            Font::Table,
            Font::Id
        )
        .name(Name::runtime("FK_2e303c3a712662f1fc2a4d0aad6"))
        .on_delete(ForeignKeyAction::Cascade)
        .on_update(ForeignKeyAction::Cascade)
        .to_string(),
        [
            r#"ALTER TABLE "schema"."character" ADD CONSTRAINT "FK_2e303c3a712662f1fc2a4d0aad6""#,
            r#"FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#,
            r#"ON DELETE CASCADE ON UPDATE CASCADE"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:sql.ddl.foreign-key+6/test]    all three check-timing states render, after the
// referential actions, and the default renders only when it is asked for
#[test]
fn deferrability_renders_after_the_referential_actions() {
    let created = |deferrability| {
        ForeignKey::create(Char::Table, Char::FontId, Font::Table, Font::Id)
            .name(Name::runtime("fk_font"))
            .on_delete(ForeignKeyAction::Cascade)
            .deferrability(deferrability)
            .to_string()
    };

    for (deferrability, tail) in [
        (Deferrability::NotDeferrable, "NOT DEFERRABLE"),
        (
            Deferrability::DeferrableInitiallyImmediate,
            "DEFERRABLE INITIALLY IMMEDIATE",
        ),
        (
            Deferrability::DeferrableInitiallyDeferred,
            "DEFERRABLE INITIALLY DEFERRED",
        ),
    ] {
        assert_eq!(
            created(deferrability),
            [
                r#"ALTER TABLE "character" ADD CONSTRAINT "fk_font""#,
                r#"FOREIGN KEY ("font_id") REFERENCES "font" ("id") ON DELETE CASCADE"#,
                tail,
            ]
            .join(" ")
        );
    }

    // Unset, the clause is absent rather than rendered as the server default.
    assert_eq!(
        ForeignKey::create(Char::Table, Char::FontId, Font::Table, Font::Id)
            .name(Name::runtime("fk_font"))
            .to_string(),
        [
            r#"ALTER TABLE "character" ADD CONSTRAINT "fk_font""#,
            r#"FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:sql.ddl.foreign-key+6/test]
#[test]
fn drop_1() {
    assert_eq!(
        ForeignKey::drop(Char::Table, Name::runtime("FK_2e303c3a712662f1fc2a4d0aad6")).to_string(),
        r#"ALTER TABLE "character" DROP CONSTRAINT "FK_2e303c3a712662f1fc2a4d0aad6""#
    );
}

#[test]
fn drop_2() {
    assert_eq!(
        ForeignKey::drop(
            (Name::runtime("schema"), Char::Table),
            Name::runtime("FK_2e303c3a712662f1fc2a4d0aad6")
        )
        .to_string(),
        r#"ALTER TABLE "schema"."character" DROP CONSTRAINT "FK_2e303c3a712662f1fc2a4d0aad6""#
    );
}
