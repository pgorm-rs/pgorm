use super::*;
use crate::oracle::{assert_eq, assert_eq_unparsed};

// [spec:pgorm:req:sql.ddl.create-table+7/test]
// [spec:pgorm:req:sql.ddl.column-def+4/test]
#[test]
// [spec:pgorm:def:sql.render.ddl.types+5/test]
fn create_1() {
    assert_eq!(
        Table::create(Glyph::Table)
            .col(
                ColumnDef::new(Glyph::Id)
                    .integer()
                    .not_null()
                    .auto_increment()
                    .primary_key()
            )
            .col(ColumnDef::new(Glyph::Aspect).double().not_null())
            .col(ColumnDef::new(Glyph::Image).text())
            .to_string(),
        [
            r#"CREATE TABLE "glyph" ("#,
            r#""id" serial NOT NULL PRIMARY KEY,"#,
            r#""aspect" double precision NOT NULL,"#,
            r#""image" text"#,
            r#")"#,
        ]
        .join(" ")
    );
}

#[test]
fn create_2() {
    assert_eq!(
        Table::create(Font::Table)
            .col(
                ColumnDef::new(Font::Id)
                    .integer()
                    .not_null()
                    .primary_key()
                    .auto_increment()
            )
            .col(ColumnDef::new(Font::Name).string().not_null())
            .col(ColumnDef::new(Font::Variant).string_len(255).not_null())
            .col(ColumnDef::new(Font::Language).string_len(255).not_null())
            .to_string(),
        [
            r#"CREATE TABLE "font" ("#,
            r#""id" serial NOT NULL PRIMARY KEY,"#,
            r#""name" varchar NOT NULL,"#,
            r#""variant" varchar(255) NOT NULL,"#,
            r#""language" varchar(255) NOT NULL"#,
            r#")"#,
        ]
        .join(" ")
    );
}

#[test]
fn create_3() {
    assert_eq!(
        Table::create(Char::Table)
            .if_not_exists()
            .col(
                ColumnDef::new(Char::Id)
                    .integer()
                    .not_null()
                    .primary_key()
                    .auto_increment()
            )
            .col(ColumnDef::new(Char::FontSize).integer().not_null())
            .col(ColumnDef::new(Char::Character).string_len(255).not_null())
            .col(ColumnDef::new(Char::SizeW).integer().not_null())
            .col(ColumnDef::new(Char::SizeH).integer().not_null())
            .col(
                ColumnDef::new(Char::FontId)
                    .integer()
                    .default(Value::Int(None))
            )
            .foreign_key(
                ForeignKey::create(Char::Table, Char::FontId, Font::Table, Font::Id)
                    .name(Name::runtime("FK_2e303c3a712662f1fc2a4d0aad6"))
                    .on_delete(ForeignKeyAction::Cascade)
                    .on_update(ForeignKeyAction::Cascade)
                    .to_owned()
            )
            .to_string(),
        [
            r#"CREATE TABLE IF NOT EXISTS "character" ("#,
            r#""id" serial NOT NULL PRIMARY KEY,"#,
            r#""font_size" integer NOT NULL,"#,
            r#""character" varchar(255) NOT NULL,"#,
            r#""size_w" integer NOT NULL,"#,
            r#""size_h" integer NOT NULL,"#,
            r#""font_id" integer DEFAULT NULL,"#,
            r#"CONSTRAINT "FK_2e303c3a712662f1fc2a4d0aad6""#,
            r#"FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#,
            r#"ON DELETE CASCADE ON UPDATE CASCADE"#,
            r#")"#,
        ]
        .join(" ")
    );
}

#[test]
fn create_4() {
    assert_eq!(
        Table::create(Glyph::Table)
            .col(ColumnDef::new(Glyph::Image).named(Glyph::Aspect))
            .to_string(),
        r#"CREATE TABLE "glyph" ( "image" aspect )"#
    );
}

// [spec:pgorm:req:sql.ddl.column-types+4/test]
#[test]
fn create_5() {
    assert_eq!(
        Table::create(Glyph::Table)
            .col(ColumnDef::new(Glyph::Image).json())
            .col(ColumnDef::new(Glyph::Aspect).json_binary())
            .to_string(),
        [
            r#"CREATE TABLE "glyph" ("#,
            r#""image" json,"#,
            r#""aspect" jsonb"#,
            r#")"#,
        ]
        .join(" ")
    );
}

#[test]
fn create_6() {
    assert_eq_unparsed!(
        Table::create(Glyph::Table)
            .col(
                ColumnDef::new(Glyph::Id)
                    .integer()
                    .not_null()
                    .raw_suffix("ANYTHING I WANT TO SAY")
            )
            .to_string(),
        [
            r#"CREATE TABLE "glyph" ("#,
            r#""id" integer NOT NULL ANYTHING I WANT TO SAY"#,
            r#")"#,
        ]
        .join(" ")
    );
}

#[test]
fn create_7() {
    assert_eq!(
        Table::create(Glyph::Table)
            .col(
                ColumnDef::new(Glyph::Aspect)
                    .interval(IntervalSpec::Any(None))
                    .not_null()
            )
            .to_string(),
        [
            r#"CREATE TABLE "glyph" ("#,
            r#""aspect" interval NOT NULL"#,
            r#")"#,
        ]
        .join(" ")
    );
}

#[test]
fn create_8() {
    assert_eq!(
        Table::create(Glyph::Table)
            .col(
                ColumnDef::new(Glyph::Aspect)
                    .interval(IntervalSpec::Fields(PgInterval::YearToMonth))
                    .not_null()
            )
            .to_string(),
        [
            r#"CREATE TABLE "glyph" ("#,
            r#""aspect" interval YEAR TO MONTH NOT NULL"#,
            r#")"#,
        ]
        .join(" ")
    );
}

#[test]
fn create_9() {
    assert_eq!(
        Table::create(Glyph::Table)
            .col(
                ColumnDef::new(Glyph::Aspect)
                    .interval(IntervalSpec::Any(Some(IntervalPrecision::P4)))
                    .not_null()
            )
            .to_string(),
        [
            r#"CREATE TABLE "glyph" ("#,
            r#""aspect" interval(4) NOT NULL"#,
            r#")"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:def:sql.types.column-type+7/test]    a precision rides on the second-bearing
// field, the only place PostgreSQL takes one
#[test]
fn create_10() {
    assert_eq!(
        Table::create(Glyph::Table)
            .col(
                ColumnDef::new(Glyph::Aspect)
                    .interval(IntervalSpec::Fields(PgInterval::HourToSecond(Some(
                        IntervalPrecision::P3
                    ))))
                    .not_null()
            )
            .to_string(),
        [
            r#"CREATE TABLE "glyph" ("#,
            r#""aspect" interval HOUR TO SECOND(3) NOT NULL"#,
            r#")"#,
        ]
        .join(" ")
    );
}

#[test]
fn create_11() {
    assert_eq!(
        Table::create(Char::Table)
            .col(
                ColumnDef::new(Char::CreatedAt)
                    .timestamp_with_time_zone()
                    .not_null()
            )
            .to_string(),
        [
            r#"CREATE TABLE "character" ("#,
            r#""created_at" timestamp with time zone NOT NULL"#,
            r#")"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:sql.ddl.column-types+4/test]
#[test]
fn create_12() {
    assert_eq!(
        Table::create(BinaryType::Table)
            .col(ColumnDef::new(BinaryType::BinaryLen).bytea())
            .col(ColumnDef::new(BinaryType::Binary).bytea())
            .to_string(),
        [
            r#"CREATE TABLE "binary_type" ("#,
            r#""binlen" bytea,"#,
            r#""bin" bytea"#,
            r#")"#,
        ]
        .join(" ")
    );
}

#[test]
fn create_14() {
    assert_eq!(
        Table::create((Name::runtime("schema"), Glyph::Table))
            .col(ColumnDef::new(Glyph::Image).named(Glyph::Aspect))
            .to_string(),
        [
            r#"CREATE TABLE "schema"."glyph" ("#,
            r#""image" aspect"#,
            r#")"#,
        ]
        .join(" ")
    );
}

#[test]
fn create_15() {
    assert_eq!(
        Table::create(Glyph::Table)
            .col(ColumnDef::new(Glyph::Image).json())
            .col(ColumnDef::new(Glyph::Aspect).json_binary())
            .index(
                Index::create(Glyph::Table, Glyph::Aspect)
                    .unique()
                    .nulls_not_distinct()
                    .name(Name::runtime("idx-glyph-aspect-image"))
                    .col(Glyph::Image)
                    .to_owned()
            )
            .to_string(),
        [
            r#"CREATE TABLE "glyph" ("#,
            r#""image" json,"#,
            r#""aspect" jsonb,"#,
            r#"CONSTRAINT "idx-glyph-aspect-image" UNIQUE NULLS NOT DISTINCT ("aspect", "image")"#,
            r#")"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:sql.ddl.drop-rename-truncate+4/test]
#[test]
fn drop_1() {
    assert_eq!(
        Table::drop(Glyph::Table)
            .table(Char::Table)
            .cascade()
            .to_string(),
        r#"DROP TABLE "glyph", "character" CASCADE"#
    );
}

#[test]
fn drop_2() {
    assert_eq!(
        Table::drop((Name::runtime("schema1"), Glyph::Table))
            .table((Name::runtime("schema2"), Char::Table))
            .cascade()
            .to_string(),
        r#"DROP TABLE "schema1"."glyph", "schema2"."character" CASCADE"#
    );
}

#[test]
fn truncate_1() {
    assert_eq!(
        Table::truncate(Font::Table).to_string(),
        r#"TRUNCATE TABLE "font""#
    );
}

#[test]
fn truncate_2() {
    assert_eq!(
        Table::truncate((Name::runtime("schema"), Font::Table)).to_string(),
        r#"TRUNCATE TABLE "schema"."font""#
    );
}

// [spec:pgorm:req:sql.ddl.alter-table+4/test]
#[test]
fn alter_1() {
    assert_eq!(
        Table::alter(Font::Table)
            .add_column(
                ColumnDef::new(Name::runtime("new_col"))
                    .integer()
                    .not_null()
                    .default(100)
            )
            .to_string(),
        r#"ALTER TABLE "font" ADD COLUMN "new_col" integer NOT NULL DEFAULT 100"#
    );
}

// [spec:pgorm:req:sql.ddl.alter-table+4/test]
#[test]
fn alter_2() {
    assert_eq!(
        Table::alter(Font::Table)
            .modify_column(
                ColumnDef::new(Name::runtime("new_col"))
                    .big_integer()
                    .default(999)
            )
            .to_string(),
        [
            r#"ALTER TABLE "font""#,
            r#"ALTER COLUMN "new_col" TYPE bigint,"#,
            r#"ALTER COLUMN "new_col" SET DEFAULT 999"#,
        ]
        .join(" ")
    );
}

#[test]
fn alter_3() {
    assert_eq!(
        Table::rename_column(
            Font::Table,
            Name::runtime("new_col"),
            Name::runtime("new_column")
        )
        .to_string(),
        r#"ALTER TABLE "font" RENAME COLUMN "new_col" TO "new_column""#
    );
}

#[test]
fn alter_4() {
    assert_eq!(
        Table::alter(Font::Table)
            .drop_column(Name::runtime("new_column"))
            .to_string(),
        r#"ALTER TABLE "font" DROP COLUMN "new_column""#
    );
}

#[test]
fn alter_5() {
    assert_eq!(
        Table::rename_column(
            (Name::runtime("schema"), Font::Table),
            Name::runtime("new_col"),
            Name::runtime("new_column")
        )
        .to_string(),
        r#"ALTER TABLE "schema"."font" RENAME COLUMN "new_col" TO "new_column""#
    );
}

// [spec:pgorm:req:sql.ddl.alter-table+4/test]    a rename is a statement of its own, so it
// cannot join the comma-separated options
#[test]
fn alter_7() {
    assert_eq!(
        Table::alter(Font::Table)
            .add_column(ColumnDef::new(Name::runtime("new_col")).integer())
            .drop_column(Font::Name)
            .to_string(),
        r#"ALTER TABLE "font" ADD COLUMN "new_col" integer, DROP COLUMN "name""#
    );
}

#[test]
fn alter_8() {
    assert_eq!(
        Table::alter(Font::Table)
            .modify_column(ColumnDef::new(Font::Language).null())
            .to_string(),
        [
            r#"ALTER TABLE "font""#,
            r#"ALTER COLUMN "language" DROP NOT NULL"#,
        ]
        .join(" ")
    );
}

#[test]
fn alter_9() {
    // https://dbfiddle.uk/98Vd8pmn
    assert_eq!(
        Table::alter(Glyph::Table)
            .modify_column(
                ColumnDef::new(Glyph::Aspect)
                    .integer()
                    .auto_increment()
                    .not_null()
                    .unique_key()
                    .primary_key()
            )
            .to_string(),
        [
            r#"ALTER TABLE "glyph""#,
            r#"ALTER COLUMN "aspect" TYPE integer,"#,
            r#"ALTER COLUMN "aspect" SET NOT NULL,"#,
            r#"ADD UNIQUE ("aspect"),"#,
            r#"ADD PRIMARY KEY ("aspect")"#,
        ]
        .join(" ")
    );
}

#[test]
fn alter_10() {
    // https://dbfiddle.uk/BeiZPvBe
    assert_eq!(
        Table::alter(Glyph::Table)
            .add_column(
                ColumnDef::new(Glyph::Aspect)
                    .integer()
                    .auto_increment()
                    .not_null()
                    .unique_key()
                    .primary_key()
            )
            .to_string(),
        [
            r#"ALTER TABLE "glyph""#,
            r#"ADD COLUMN "aspect" serial NOT NULL UNIQUE PRIMARY KEY"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:sql.ddl.drop-rename-truncate+4/test]
#[test]
fn rename_1() {
    assert_eq!(
        Table::rename(Font::Table, Name::runtime("font_new")).to_string(),
        r#"ALTER TABLE "font" RENAME TO "font_new""#
    );
}

#[test]
fn rename_2() {
    assert_eq!(
        Table::rename(
            (Name::runtime("schema"), Font::Table),
            Name::runtime("font_new")
        )
        .to_string(),
        r#"ALTER TABLE "schema"."font" RENAME TO "font_new""#
    );
}

#[test]
fn create_with_check_constraint() {
    assert_eq!(
        Table::create(Glyph::Table)
            .col(
                ColumnDef::new(Glyph::Id)
                    .integer()
                    .not_null()
                    .check(Expr::col(Glyph::Id).gt(10))
            )
            .check(Expr::col(Glyph::Id).lt(20))
            .check(Expr::col(Glyph::Id).ne(15))
            .to_string(),
        r#"CREATE TABLE "glyph" ( "id" integer NOT NULL CHECK ("id" > 10), CHECK ("id" < 20), CHECK ("id" <> 15) )"#,
    );
}

#[test]
fn alter_with_check_constraint() {
    assert_eq!(
        Table::alter(Glyph::Table)
            .add_column(
                ColumnDef::new(Glyph::Aspect)
                    .integer()
                    .not_null()
                    .default(101)
                    .check(Expr::col(Glyph::Aspect).gt(100))
            )
            .to_string(),
        r#"ALTER TABLE "glyph" ADD COLUMN "aspect" integer NOT NULL DEFAULT 101 CHECK ("aspect" > 100)"#,
    );
}

#[test]
fn create_16() {
    assert_eq!(
        Table::create(Glyph::Table)
            .col(
                ColumnDef::new(Glyph::Id)
                    .integer()
                    .not_null()
                    .auto_increment()
                    .primary_key()
            )
            .col(ColumnDef::new(Glyph::Tokens).ltree())
            .to_string(),
        [
            r#"CREATE TABLE "glyph" ("#,
            r#""id" serial NOT NULL PRIMARY KEY,"#,
            r#""tokens" ltree"#,
            r#")"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:sql.ddl.create-table+7/test]
#[test]
fn embedded_index_is_the_only_primary_key_spelling() {
    let table = |index: IndexCreateStatement| {
        Table::create(Glyph::Table)
            .col(ColumnDef::new(Glyph::Id).integer().not_null())
            .col(ColumnDef::new(Glyph::Image).string().not_null())
            .primary_key(index)
            .to_string()
    };
    let expected = [
        r#"CREATE TABLE "glyph" ("#,
        r#""id" integer NOT NULL,"#,
        r#""image" varchar NOT NULL,"#,
        r#"CONSTRAINT "pk-glyph" PRIMARY KEY ("id", "image")"#,
        r#")"#,
    ]
    .join(" ");
    let index = || {
        Index::create(Glyph::Table, Glyph::Id)
            .name(Name::runtime("pk-glyph"))
            .col(Glyph::Image)
            .to_owned()
    };

    assert_eq!(table(index()), expected);
    assert_eq!(table(index().primary().to_owned()), expected);
    assert_eq!(table(index().unique().to_owned()), expected);
}

// [spec:pgorm:req:sql.ddl.alter-table+4/test]    a foreign key embeds by value, so the source
// survives only where the call site cloned it
#[test]
fn alter_embeds_its_foreign_key_by_value() {
    let key = TableForeignKey::new(Char::Table, Char::FontId, Font::Table, Font::Id)
        .name(Name::runtime("fk-character-font_id"))
        .to_owned();

    assert_eq!(
        Table::alter(Char::Table)
            .add_foreign_key(key.clone())
            .to_string(),
        [
            r#"ALTER TABLE "character" ADD CONSTRAINT "fk-character-font_id""#,
            r#"FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#,
        ]
        .join(" ")
    );
    assert_eq!(
        Table::alter(Char::Table).add_foreign_key(key).to_string(),
        [
            r#"ALTER TABLE "character" ADD CONSTRAINT "fk-character-font_id""#,
            r#"FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:sql.ddl.column-def+4/test]    a generated column is stored, and the virtual
// spelling it no longer has a constructor for is one the grammar refuses
#[test]
fn generated_column_is_always_stored() {
    assert_eq!(
        Table::create(Glyph::Table)
            .col(
                ColumnDef::new(Glyph::Aspect)
                    .integer()
                    .generated(Expr::col(Glyph::Id).mul(2))
            )
            .to_string(),
        r#"CREATE TABLE "glyph" ( "aspect" integer GENERATED ALWAYS AS ("id" * 2) STORED )"#
    );

    // Why there is no non-stored spelling to ask for: PostgreSQL's own parser
    // rejects it, so the render that used to emit it could only ever fail at
    // the server.
    assert!(
        crate::oracle::parses(
            r#"CREATE TABLE "glyph" ( "aspect" integer GENERATED ALWAYS AS ("id" * 2) VIRTUAL )"#
        )
        .is_err(),
        "the grammar accepts VIRTUAL, so this refusal wants revisiting"
    );
}
