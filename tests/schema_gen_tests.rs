#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, setup::*};

use pgorm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityName,
    EntityTrait, Iterable, Schema, entity::prelude::*,
};
use pgorm_query::{ColumnDef, ColumnSpec, ColumnType, QueryBuilder, TableCreateStatement};
use pretty_assertions::assert_eq;

mod factory {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "factory")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        pub name: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

mod gadget {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "gadget")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        #[pgorm(indexed)]
        pub batch: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

mod widget {
    use pgorm::entity::prelude::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
    #[pgorm(rs_type = "String", db_type = "Enum", enum_name = "widget_grade")]
    pub enum Grade {
        #[pgorm(string_value = "Gold")]
        Gold,
        #[pgorm(string_value = "Silver")]
        Silver,
        #[pgorm(string_value = "Bronze")]
        Bronze,
    }

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(
        table_name = "widget",
        schema_name = "public",
        comment = "one row per widget"
    )]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        #[pgorm(unique)]
        pub code: String,
        #[pgorm(indexed)]
        pub batch: i32,
        pub note: Option<String>,
        #[pgorm(default_value = "unknown", comment = "supplier of record")]
        pub origin: String,
        pub grade: Grade,
        pub spare_grade: Grade,
        pub factory_id: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[pgorm(
            belongs_to = "super::factory::Entity",
            from = "Column::FactoryId",
            to = "super::factory::Column::Id"
        )]
        Factory,
        #[pgorm(has_many = "super::widget_tag::Entity")]
        WidgetTag,
    }

    impl Related<super::widget_tag::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::WidgetTag.def()
        }
    }

    impl Related<super::factory::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Factory.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

mod widget_tag {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "widget_tag")]
    pub struct Model {
        #[pgorm(primary_key, auto_increment = false)]
        pub widget_id: i32,
        #[pgorm(primary_key, auto_increment = false)]
        pub tag: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[pgorm(
            belongs_to = "super::widget::Entity",
            from = "Column::WidgetId",
            to = "super::widget::Column::Id"
        )]
        Widget,
    }

    impl Related<super::widget::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Widget.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

/// A bare (unqualified) entity whose comments contain the one character the
/// comment literal has to escape.
mod quirk {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "quirk", comment = "it's a table")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        #[pgorm(comment = "don't drop it")]
        pub note: String,
        pub plain: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// An `ActiveEnum` whose `db_type()` is not `ColumnType::Enum`.
#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[pgorm(rs_type = "String", db_type = "String(StringLen::N(1))")]
pub enum Size {
    #[pgorm(string_value = "B")]
    Big,
    #[pgorm(string_value = "S")]
    Small,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Flags {
    not_null: bool,
    unique: bool,
    primary_key: bool,
    auto_increment: bool,
    default: bool,
    comment: Option<String>,
}

fn column<'a>(stmt: &'a TableCreateStatement, name: &str) -> &'a ColumnDef {
    stmt.get_columns()
        .iter()
        .find(|col| col.get_column_name() == name)
        .unwrap_or_else(|| panic!("no column {name} in the projected statement"))
}

fn flags(col: &ColumnDef) -> Flags {
    let mut flags = Flags::default();
    for spec in col.get_column_spec() {
        match spec {
            ColumnSpec::NotNull => flags.not_null = true,
            ColumnSpec::UniqueKey => flags.unique = true,
            ColumnSpec::PrimaryKey => flags.primary_key = true,
            ColumnSpec::AutoIncrement => flags.auto_increment = true,
            ColumnSpec::Default(_) => flags.default = true,
            ColumnSpec::Comment(comment) => flags.comment = Some(comment.clone()),
            _ => {}
        }
    }
    flags
}

// [spec:pgorm:sem:schema.from-entity+1/test]    table ref, comment, per-column projection, single-column key, belongs-to foreign keys
#[test]
fn create_table_from_entity_projects_columns() {
    let schema = Schema::new();
    let stmt = schema.create_table_from_entity(widget::Entity);

    // Table ref comes from `entity.table_ref()`, hence schema-qualified.
    assert_eq!(
        stmt.get_table_name(),
        Some(&widget::Entity.table_ref()),
        "the statement targets the entity's table ref"
    );
    assert_eq!(
        stmt.get_comment().map(String::as_str),
        Some("one row per widget")
    );

    // One column per `Column` variant, in declaration order.
    let names: Vec<String> = stmt
        .get_columns()
        .iter()
        .map(|col| col.get_column_name())
        .collect();
    assert_eq!(
        names,
        widget::Column::iter()
            .map(|col| col.to_string())
            .collect::<Vec<_>>()
    );

    // Single-column primary key: inline PRIMARY KEY plus auto_increment.
    assert!(widget::PrimaryKey::auto_increment());
    assert_eq!(
        flags(column(&stmt, "id")),
        Flags {
            not_null: true,
            primary_key: true,
            auto_increment: true,
            ..Default::default()
        }
    );
    assert!(
        stmt.get_indexes().is_empty(),
        "arity-1 keys emit no table-level index"
    );

    // Declared column types are carried through.
    assert_eq!(
        column(&stmt, "batch").get_column_type(),
        Some(&ColumnType::Integer)
    );
    assert!(matches!(
        column(&stmt, "code").get_column_type(),
        Some(ColumnType::String(_))
    ));

    // `unique` columns gain a unique key; `indexed` ones gain nothing here.
    assert!(flags(column(&stmt, "code")).unique);
    assert!(!flags(column(&stmt, "batch")).unique);

    // NOT NULL unless the column is nullable.
    assert!(flags(column(&stmt, "code")).not_null);
    assert!(!flags(column(&stmt, "note")).not_null);

    // Default value and column comment ride along.
    let origin = flags(column(&stmt, "origin"));
    assert!(origin.default);
    assert_eq!(origin.comment.as_deref(), Some("supplier of record"));
    assert_eq!(flags(column(&stmt, "code")).comment, None);

    // `ColumnType::Enum` is rewritten to a custom type reference naming the enum.
    for name in ["grade", "spare_grade"] {
        match column(&stmt, name).get_column_type() {
            Some(ColumnType::Custom(iden)) => assert_eq!(iden.to_string(), "widget_grade"),
            other => panic!("expected a custom type reference for {name}, got {other:?}"),
        }
    }
    assert!(matches!(
        widget::Column::Grade.def().get_column_type(),
        ColumnType::Enum { .. }
    ));

    // Foreign keys come from the belongs-to side only.
    let fks = stmt.get_foreign_key_create_stmts();
    assert_eq!(
        fks.len(),
        1,
        "only the belongs-to relation produces a constraint: {:?}",
        fks.iter()
            .map(|fk| fk.to_string(QueryBuilder))
            .collect::<Vec<_>>()
    );
    let fk = fks[0].to_string(QueryBuilder);
    assert!(fk.contains("\"factory_id\""), "{fk}");
    assert!(fk.contains("\"factory\""), "{fk}");
    assert!(
        !fk.contains("widget_tag"),
        "the has_many (owner) side produces nothing: {fk}"
    );
    assert!(widget::Relation::WidgetTag.def().is_owner);
    assert!(!widget::Relation::Factory.def().is_owner);
}

// [spec:pgorm:sem:schema.from-entity+1/test]    composite keys emit a table-level pk-{table} index instead of the inline flag
#[test]
fn create_table_composite_key_emits_index() {
    let schema = Schema::new();
    let stmt = schema.create_table_from_entity(widget_tag::Entity);

    for name in ["widget_id", "tag"] {
        let flags = flags(column(&stmt, name));
        assert!(
            !flags.primary_key,
            "composite key columns carry no inline PRIMARY KEY: {name}"
        );
        assert!(!flags.auto_increment);
    }

    let indexes = stmt.get_indexes();
    assert_eq!(indexes.len(), 1);
    let rendered = stmt.to_string(QueryBuilder);
    assert!(rendered.contains("\"pk-widget_tag\""), "{rendered}");
    assert!(rendered.contains("PRIMARY KEY"), "{rendered}");
    assert!(
        rendered.contains("\"widget_id\", \"tag\""),
        "both key columns are in the table-level index: {rendered}"
    );
}

// [spec:pgorm:sem:schema.from-entity+1/test]    the entity comment first, then the commented
// columns in Column order, targeting entity.table_ref() with the text quoted
#[test]
fn create_comments_from_entity_emits_statements() {
    let schema = Schema::new();

    let rendered: Vec<String> = schema
        .create_comments_from_entity(widget::Entity)
        .iter()
        .map(|stmt| stmt.to_string(QueryBuilder))
        .collect();
    assert_eq!(
        rendered,
        vec![
            r#"COMMENT ON TABLE "public"."widget" IS 'one row per widget'"#.to_owned(),
            r#"COMMENT ON COLUMN "public"."widget"."origin" IS 'supplier of record'"#.to_owned(),
        ],
        "comments target the same qualified name the table projection uses"
    );

    // The text still rides on the create statement, where it stays inert.
    let table = schema.create_table_from_entity(widget::Entity);
    assert_eq!(
        table.get_comment().map(String::as_str),
        Some("one row per widget")
    );
    let create = table.to_string(QueryBuilder);
    assert!(!create.contains("one row per widget"), "{create}");
    assert!(!create.contains("supplier of record"), "{create}");

    // An entity with no schema stays unqualified, and a quote in the text is doubled.
    let rendered: Vec<String> = schema
        .create_comments_from_entity(quirk::Entity)
        .iter()
        .map(|stmt| stmt.to_string(QueryBuilder))
        .collect();
    assert_eq!(
        rendered,
        vec![
            r#"COMMENT ON TABLE "quirk" IS 'it''s a table'"#.to_owned(),
            r#"COMMENT ON COLUMN "quirk"."note" IS 'don''t drop it'"#.to_owned(),
        ]
    );

    assert!(
        schema
            .create_comments_from_entity(factory::Entity)
            .is_empty(),
        "an entity that declares no comment produces no statements"
    );
}

// [spec:pgorm:sem:schema.from-entity.index+1/test]    one statement per indexed column, named idx-{table}-{column}
#[test]
fn create_index_from_entity_names_indexed_columns() {
    let schema = Schema::new();

    let stmts = schema.create_index_from_entity(widget::Entity);
    assert_eq!(stmts.len(), 1, "only `batch` is flagged `indexed`");
    assert_eq!(
        stmts[0].to_string(QueryBuilder),
        r#"CREATE INDEX "idx-widget-batch" ON "public"."widget" ("batch")"#
    );

    // Unique columns are not covered here: `code` gets a column-level unique key from
    // the table projection, not an index statement of its own.
    let table = schema.create_table_from_entity(widget::Entity);
    assert!(flags(column(&table, "code")).unique);
    assert!(
        !stmts
            .iter()
            .any(|stmt| stmt.to_string(QueryBuilder).contains("code")),
        "uniqueness is not emitted as a separate index"
    );

    // No indexed column at all yields an empty Vec.
    assert!(
        schema.create_index_from_entity(factory::Entity).is_empty(),
        "an entity with no indexed column produces no statements"
    );
}

// [spec:pgorm:sem:schema.from-entity.index+1/test]    the index targets entity.table_ref(), so it is schema-qualified exactly when the entity declares one
#[test]
fn create_index_from_entity_uses_table_ref() {
    let schema = Schema::new();

    assert_eq!(widget::Entity.schema_name(), Some("public"));
    let qualified = schema.create_index_from_entity(widget::Entity);
    assert_eq!(
        qualified[0].to_string(QueryBuilder),
        r#"CREATE INDEX "idx-widget-batch" ON "public"."widget" ("batch")"#
    );
    let table = schema.create_table_from_entity(widget::Entity);
    assert_eq!(
        table.get_table_name(),
        Some(&widget::Entity.table_ref()),
        "index and table projections agree on the target ref"
    );

    assert_eq!(gadget::Entity.schema_name(), None);
    let bare = schema.create_index_from_entity(gadget::Entity);
    assert_eq!(
        bare[0].to_string(QueryBuilder),
        r#"CREATE INDEX "idx-gadget-batch" ON "gadget" ("batch")"#
    );
}

// [spec:pgorm:sem:schema.from-entity.enum+1/test]    one statement per enum column, variant order preserved, duplicates kept
#[test]
fn create_enum_from_entity_emits_per_column() {
    let schema = Schema::new();

    let stmts = schema.create_enum_from_entity(widget::Entity);
    let rendered: Vec<String> = stmts
        .iter()
        .map(|stmt| stmt.to_string(QueryBuilder))
        .collect();
    assert_eq!(
        rendered,
        vec![
            r#"CREATE TYPE "widget_grade" AS ENUM ('Gold', 'Silver', 'Bronze')"#.to_owned(),
            r#"CREATE TYPE "widget_grade" AS ENUM ('Gold', 'Silver', 'Bronze')"#.to_owned(),
        ],
        "two columns sharing one enum type yield two identical statements"
    );

    assert!(
        schema.create_enum_from_entity(factory::Entity).is_empty(),
        "an entity with no enum column produces no statements"
    );
}

// [spec:pgorm:sem:schema.from-entity.enum+1/test]    the single-ActiveEnum form builds the same statement from A::db_type()
#[test]
fn create_enum_from_active_enum_uses_db_type() {
    let schema = Schema::new();

    assert_eq!(
        schema
            .create_enum_from_active_enum::<widget::Grade>()
            .expect("Grade resolves to ColumnType::Enum")
            .to_string(QueryBuilder),
        r#"CREATE TYPE "widget_grade" AS ENUM ('Gold', 'Silver', 'Bronze')"#
    );
    assert!(matches!(
        widget::Grade::db_type().get_column_type(),
        ColumnType::Enum { .. }
    ));
}

// [spec:pgorm:sem:schema.from-entity.enum+1/test]    errors when the resolved column type is not an enum
#[test]
fn create_enum_from_active_enum_errs_non_enum() {
    assert!(!matches!(
        Size::db_type().get_column_type(),
        ColumnType::Enum { .. }
    ));
    let err = Schema::new()
        .create_enum_from_active_enum::<Size>()
        .expect_err("a String-backed ActiveEnum has no type to create");
    assert!(
        matches!(err, DbErr::Type(ref msg) if msg.contains("not backed by a database enum")),
        "expected a type error naming the enum, got {err:?}"
    );
}

// [spec:pgorm:sem:schema.from-entity+1/test]    the projected DDL is accepted by Postgres and enforces what it declares
// [spec:pgorm:sem:schema.from-entity.index+1/test]    the schema-qualified index executes and reaches pg_indexes under its generated name
// [spec:pgorm:sem:schema.from-entity.enum+1/test]    the projected type is a usable Postgres enum
#[pgorm_macros::test]
async fn generated_schema_executes_on_postgres() -> Result<(), DbErr> {
    let ctx = TestContext::new("schema_gen_executes_schemagen").await;
    let db = ctx.db.get().await?;
    let schema = Schema::new();

    // One CREATE TYPE per enum column: the duplicate is the caller's problem.
    let enums = schema.create_enum_from_entity(widget::Entity);
    db.execute(&enums[0].to_string(QueryBuilder), &[]).await?;
    let duplicate = db
        .execute(&enums[1].to_string(QueryBuilder), &[])
        .await
        .expect_err("the second statement re-creates the same type");
    assert!(matches!(duplicate, DbErr::Postgres(_)));

    for entity_stmt in [
        schema.create_table_from_entity(factory::Entity),
        schema.create_table_from_entity(widget::Entity),
        schema.create_table_from_entity(widget_tag::Entity),
    ] {
        db.execute(&entity_stmt.build(QueryBuilder), &[]).await?;
    }
    for index in schema.create_index_from_entity(widget::Entity) {
        let sql = index.to_string(QueryBuilder);
        assert!(sql.contains(r#"ON "public"."widget""#), "{sql}");
        db.execute(&sql, &[]).await?;
    }

    let indexes: Vec<String> = db
        .query_all(
            "SELECT indexname FROM pg_indexes WHERE schemaname = 'public' AND tablename = 'widget'",
            &[],
        )
        .await?
        .iter()
        .map(|row| row.get::<_, String>(0))
        .collect();
    assert!(
        indexes.contains(&"idx-widget-batch".to_owned()),
        "the generated index is on the schema-qualified table: {indexes:?}"
    );

    for comment in schema.create_comments_from_entity(widget::Entity) {
        db.execute(&comment.build(QueryBuilder), &[]).await?;
    }
    let table_comment: Option<String> = db
        .query_one(
            "SELECT obj_description('public.widget'::regclass, 'pg_class')",
            &[],
        )
        .await?
        .get(0);
    assert_eq!(
        table_comment.as_deref(),
        Some("one row per widget"),
        "the qualified comment target resolves to the same table"
    );

    let factory = factory::ActiveModel {
        name: Set("Acme".to_owned()),
        ..Default::default()
    }
    .insert(&db)
    .await?;

    // `origin` is omitted, so the DDL default has to supply it; `note` is nullable.
    let widget = widget::ActiveModel {
        code: Set("W-1".to_owned()),
        batch: Set(7),
        grade: Set(widget::Grade::Gold),
        spare_grade: Set(widget::Grade::Bronze),
        factory_id: Set(factory.id),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    assert_eq!(widget.origin, "unknown");
    assert_eq!(widget.note, None);
    assert_eq!(widget.grade, widget::Grade::Gold);

    // The unique key on `code` is enforced.
    let unique = widget::ActiveModel {
        code: Set("W-1".to_owned()),
        batch: Set(8),
        grade: Set(widget::Grade::Silver),
        spare_grade: Set(widget::Grade::Silver),
        factory_id: Set(factory.id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect_err("code is unique");
    assert!(matches!(
        unique.sql_err(),
        Some(pgorm::SqlErr::UniqueConstraintViolation(_))
    ));

    // The belongs-to foreign key is enforced.
    let fk = widget::ActiveModel {
        code: Set("W-2".to_owned()),
        batch: Set(9),
        grade: Set(widget::Grade::Silver),
        spare_grade: Set(widget::Grade::Silver),
        factory_id: Set(factory.id + 1_000),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect_err("factory_id has no matching factory");
    assert!(matches!(
        fk.sql_err(),
        Some(pgorm::SqlErr::ForeignKeyConstraintViolation(_))
    ));

    // The composite primary key is enforced across both columns.
    let tag = widget_tag::ActiveModel {
        widget_id: Set(widget.id),
        tag: Set("shiny".to_owned()),
    };
    tag.clone().insert(&db).await?;
    widget_tag::ActiveModel {
        widget_id: Set(widget.id),
        tag: Set("matte".to_owned()),
    }
    .insert(&db)
    .await?;
    let composite = tag
        .insert(&db)
        .await
        .expect_err("(widget_id, tag) is the primary key");
    assert!(matches!(
        composite.sql_err(),
        Some(pgorm::SqlErr::UniqueConstraintViolation(_))
    ));

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:schema.from-entity+1/test]    the comment statements execute, and only they
// attach anything: the text arrives in pg_description exactly as declared
#[pgorm_macros::test]
async fn entity_comments_land_in_pg_description() -> Result<(), DbErr> {
    const TABLE_COMMENT: &str = "SELECT obj_description('quirk'::regclass, 'pg_class')";
    const COLUMN_COMMENT: &str = "SELECT col_description('quirk'::regclass, attnum) \
         FROM pg_attribute WHERE attrelid = 'quirk'::regclass AND attname = 'note'";

    let ctx = TestContext::new("schema_gen_comments_schemagen").await;
    let db = ctx.db.get().await?;
    let schema = Schema::new();

    db.execute(
        &schema
            .create_table_from_entity(quirk::Entity)
            .build(QueryBuilder),
        &[],
    )
    .await?;

    let after_create: Option<String> = db.query_one(TABLE_COMMENT, &[]).await?.get(0);
    assert_eq!(
        after_create, None,
        "creating the table attaches no comment on its own"
    );
    let after_create: Option<String> = db.query_one(COLUMN_COMMENT, &[]).await?.get(0);
    assert_eq!(after_create, None);

    for comment in schema.create_comments_from_entity(quirk::Entity) {
        db.execute(&comment.build(QueryBuilder), &[]).await?;
    }

    let table_comment: Option<String> = db.query_one(TABLE_COMMENT, &[]).await?.get(0);
    assert_eq!(
        table_comment.as_deref(),
        Some("it's a table"),
        "the doubled quote round-trips as one quote"
    );
    let column_comment: Option<String> = db.query_one(COLUMN_COMMENT, &[]).await?.get(0);
    assert_eq!(column_comment.as_deref(), Some("don't drop it"));

    drop(db);
    ctx.delete().await;

    Ok(())
}
