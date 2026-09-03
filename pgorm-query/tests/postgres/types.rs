use super::*;
use crate::oracle::assert_eq;
use pgorm_query::extension::Type;

// [spec:pgorm:req:sql.ddl.type-enum+2/test]
#[test]
// [spec:pgorm:req:sql.render.ddl.enum-type+1/test]
fn create_1() {
    assert_eq!(
        Type::create(Font::Table)
            .values([Font::Name, Font::Variant, Font::Language])
            .to_string(),
        r#"CREATE TYPE "font" AS ENUM ('name', 'variant', 'language')"#
    );
}

#[test]
fn create_2() {
    assert_eq!(
        Type::create((Alias::new("schema"), Font::Table))
            .values([Font::Name, Font::Variant, Font::Language])
            .to_string(),
        r#"CREATE TYPE "schema"."font" AS ENUM ('name', 'variant', 'language')"#
    );
}

#[test]
fn create_3() {
    assert_eq!(
        Type::create(Tea::Enum)
            .values([Tea::EverydayTea, Tea::BreakfastTea])
            .to_string(),
        r#"CREATE TYPE "tea" AS ENUM ('EverydayTea', 'BreakfastTea')"#
    );

    // Variants are named after the SQL enum labels the assertion above expects.
    #[allow(clippy::enum_variant_names)]
    enum Tea {
        Enum,
        EverydayTea,
        BreakfastTea,
    }

    impl pgorm_query::Iden for Tea {
        fn unquoted(&self, s: &mut dyn std::fmt::Write) {
            write!(
                s,
                "{}",
                match self {
                    Self::Enum => "tea",
                    Self::EverydayTea => "EverydayTea",
                    Self::BreakfastTea => "BreakfastTea",
                }
            )
            .unwrap();
        }
    }
}

// [spec:pgorm:req:sql.ddl.type-alter-drop+3/test]
#[test]
fn drop_1() {
    assert_eq!(
        Type::drop(Font::Table).if_exists().restrict().to_string(),
        r#"DROP TYPE IF EXISTS "font" RESTRICT"#
    )
}

#[test]
fn drop_2() {
    assert_eq!(Type::drop(Font::Table).to_string(), r#"DROP TYPE "font""#);
}

#[test]
fn drop_3() {
    assert_eq!(
        Type::drop(Font::Table).if_exists().cascade().to_string(),
        r#"DROP TYPE IF EXISTS "font" CASCADE"#
    );
}

#[test]
fn drop_4() {
    assert_eq!(
        Type::drop((Alias::new("schema"), Font::Table)).to_string(),
        r#"DROP TYPE "schema"."font""#
    );
}

// [spec:pgorm:req:sql.ddl.type-alter-drop+3/test]
#[test]
fn alter_1() {
    assert_eq!(
        Type::alter(Font::Table)
            .add_value(Alias::new("weight"))
            .to_string(),
        r#"ALTER TYPE "font" ADD VALUE 'weight'"#
    )
}
#[test]
fn alter_2() {
    assert_eq!(
        Type::alter(Font::Table)
            .add_value(Alias::new("weight"))
            .before(Font::Variant)
            .to_string(),
        r#"ALTER TYPE "font" ADD VALUE 'weight' BEFORE 'variant'"#
    )
}

#[test]
fn alter_3() {
    assert_eq!(
        Type::alter(Font::Table)
            .add_value(Alias::new("weight"))
            .after(Font::Variant)
            .to_string(),
        r#"ALTER TYPE "font" ADD VALUE 'weight' AFTER 'variant'"#
    )
}

#[test]
fn alter_4() {
    assert_eq!(
        Type::alter(Font::Table)
            .rename_to(Alias::new("typeface"))
            .to_string(),
        r#"ALTER TYPE "font" RENAME TO "typeface""#
    )
}

#[test]
fn alter_5() {
    assert_eq!(
        Type::alter(Font::Table)
            .rename_value(Font::Variant, Font::Language)
            .to_string(),
        r#"ALTER TYPE "font" RENAME VALUE 'variant' TO 'language'"#
    )
}

#[test]
fn alter_6() {
    assert_eq!(
        Type::alter((Alias::new("schema"), Font::Table))
            .rename_to(Alias::new("typeface"))
            .to_string(),
        r#"ALTER TYPE "schema"."font" RENAME TO "typeface""#
    )
}
