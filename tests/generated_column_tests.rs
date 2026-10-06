#![allow(unused_imports, dead_code)]

//! `GENERATED ALWAYS AS (<expr>) { STORED | VIRTUAL }` against a live
//! PostgreSQL server.
//!
//! The render tests in pgorm-query settle that both kinds parse. What only a
//! server settles is what each kind is once the table exists — the catalog's
//! `attgenerated` says `s` or `v`, which is the whole point of writing the
//! keyword rather than leaving it to a release whose default changed — and
//! what it refuses around a column of each kind.

pub mod common;
pub use common::{TestContext, setup::*};
use pgorm::pgorm_query::{
    ColumnDef, ColumnType, Expr, Func, GeneratedKind, Index, Name, Table, TableKey, extension::Type,
};
use pgorm::{ConnectionTrait, Schema, entity::prelude::*};
use pretty_assertions::assert_eq;
use tokio_postgres::error::SqlState;

/// An order line whose total is kept on disk and whose discounted price is
/// computed on read: the two kinds side by side, each from the row's own
/// columns.
mod line {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "line")]
    pub struct Model {
        #[pgorm(primary_key, identity)]
        pub id: i32,
        pub price: i32,
        pub quantity: i32,
        #[pgorm(generated_stored = "Expr::col(Column::Price).mul(Expr::col(Column::Quantity))")]
        pub total: i32,
        #[pgorm(generated_virtual = "Expr::col(Column::Price).sub(1)")]
        pub discounted: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

fn refused_with(error: &Error, state: &SqlState) {
    match error {
        Error::Postgres(e) => assert_eq!(e.code(), Some(state), "{e}"),
        other => panic!("expected Error::Postgres, got {other:?}"),
    }
}

/// The kind each column of `table` was created as, by `pg_attribute`: `s` for
/// stored, `v` for virtual, empty for a column that is not generated.
async fn generated_kinds(
    db: &DatabaseConnection,
    table: &str,
) -> Result<Vec<(String, String)>, Error> {
    let rows = db
        .query_all(
            "SELECT attname::text, attgenerated::text FROM pg_attribute \
             WHERE attrelid = $1::text::regclass AND attnum > 0 AND NOT attisdropped ORDER BY attnum",
            &[&table],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
        .collect())
}

/// Each kind lands in the catalog as itself: the column written `STORED` is
/// stored and the one written `VIRTUAL` virtual, and both compute from the row.
// [spec:pgorm:req:sql.ddl.column-def+10/test]    each kind reaches the catalog as
// written, so neither column's kind is the server's default
#[pgorm_macros::test]
async fn each_kind_reaches_the_catalog() -> Result<(), Error> {
    let ctx = TestContext::new("generated_kind_catalog").await;
    let db = ctx.db.get().await?;

    let create = Table::create(Name::runtime("measure"))
        .col(ColumnDef::new(Name::runtime("base")).integer().not_null())
        .col(ColumnDef::new(Name::runtime("kept")).integer().generated(
            Expr::col(Name::runtime("base")).mul(2),
            GeneratedKind::Stored,
        ))
        .col(
            ColumnDef::new(Name::runtime("computed"))
                .integer()
                .generated(
                    Expr::col(Name::runtime("base")).add(1),
                    GeneratedKind::Virtual,
                ),
        )
        .to_string();
    db.batch_execute(&create).await?;
    assert_eq!(
        generated_kinds(&db, "measure").await?,
        [
            ("base".to_owned(), String::new()),
            ("kept".to_owned(), "s".to_owned()),
            ("computed".to_owned(), "v".to_owned()),
        ]
    );

    db.batch_execute("INSERT INTO measure (base) VALUES (5)")
        .await?;
    let row = db
        .query_one("SELECT kept, computed FROM measure", &[])
        .await?;
    assert_eq!((row.get::<_, i32>(0), row.get::<_, i32>(1)), (10, 6));

    let added = Table::alter(Name::runtime("measure"))
        .add_column(ColumnDef::new(Name::runtime("later")).integer().generated(
            Expr::col(Name::runtime("base")).sub(1),
            GeneratedKind::Virtual,
        ))
        .to_string();
    db.batch_execute(&added).await?;
    assert_eq!(
        generated_kinds(&db, "measure").await?.last(),
        Some(&("later".to_owned(), "v".to_owned()))
    );

    drop(db);
    ctx.delete().await;
    Ok(())
}

/// An entity's generated columns are created as their kinds, read like any
/// other column and recomputed when the row changes; a model writes them by
/// leaving them `NotSet`, and one that supplies a value is refused, as a
/// `GENERATED ALWAYS` identity is.
// [spec:pgorm:sem:schema.from-entity+7/test]    an entity's generated column is
// created as its kind, after the column it reads
// [spec:pgorm:req:entity.traits.column-def+2/test]    a generated column is read
// and recomputed, and a value written into it is refused (428C9)
#[pgorm_macros::test]
async fn entity_generated_columns_compute_on_write() -> Result<(), Error> {
    let ctx = TestContext::new("generated_entity_columns").await;
    let db = ctx.db.get().await?;

    let create = Schema::new().create_table_from_entity(line::Entity);
    assert_eq!(
        create.to_string(),
        [
            r#"CREATE TABLE "line" ( "id" integer NOT NULL GENERATED ALWAYS AS IDENTITY,"#,
            r#""price" integer NOT NULL, "quantity" integer NOT NULL,"#,
            r#""total" integer NOT NULL GENERATED ALWAYS AS ("price" * "quantity") STORED,"#,
            r#""discounted" integer NOT NULL GENERATED ALWAYS AS ("price" - 1) VIRTUAL,"#,
            r#"PRIMARY KEY ("id") )"#,
        ]
        .join(" ")
    );
    db.execute(&create.to_string(), &[]).await?;
    assert_eq!(
        generated_kinds(&db, "line").await?,
        [
            ("id".to_owned(), String::new()),
            ("price".to_owned(), String::new()),
            ("quantity".to_owned(), String::new()),
            ("total".to_owned(), "s".to_owned()),
            ("discounted".to_owned(), "v".to_owned()),
        ]
    );

    let inserted = line::ActiveModel {
        price: set(7),
        quantity: set(3),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    assert_eq!((inserted.total, inserted.discounted), (21, 6));

    let updated = line::ActiveModel {
        id: Unchanged(inserted.id),
        price: set(10),
        ..Default::default()
    }
    .update(&db)
    .await?;
    assert_eq!((updated.total, updated.discounted), (30, 9));
    assert_eq!(
        line::Entity::find().all(&db).await?,
        std::slice::from_ref(&updated)
    );

    let stored = line::ActiveModel {
        price: set(1),
        quantity: set(1),
        total: set(1),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect_err("a value written into a stored generated column");
    refused_with(&stored, &SqlState::GENERATED_ALWAYS);

    let computed = line::ActiveModel {
        id: Unchanged(updated.id),
        discounted: set(1),
        ..Default::default()
    }
    .update(&db)
    .await
    .expect_err("a value written into a virtual generated column");
    refused_with(&computed, &SqlState::GENERATED_ALWAYS);

    drop(db);
    ctx.delete().await;
    Ok(())
}

/// What PostgreSQL 18 refuses around a generated column, each by the SQLSTATE
/// the rule records: a virtual column takes no index, key or user-defined type,
/// and neither kind takes a volatile expression, another generated column, or
/// a `DEFAULT` beside it. A stored column takes the key a virtual one cannot.
// [spec:pgorm:req:sql.ddl.column-def+10/test]    the refusals around each kind,
// by SQLSTATE
#[pgorm_macros::test]
async fn server_refusals_around_generated_columns() -> Result<(), Error> {
    let ctx = TestContext::new("generated_column_refusals").await;
    let db = ctx.db.get().await?;

    let base = || {
        ColumnDef::new(Name::runtime("base"))
            .integer()
            .not_null()
            .to_owned()
    };
    let derived = |kind| {
        ColumnDef::new(Name::runtime("derived"))
            .integer()
            .generated(Expr::col(Name::runtime("base")).mul(2), kind)
            .to_owned()
    };
    let refused = async |sql: String, state: &SqlState| {
        let error = db.batch_execute(&sql).await.expect_err(&sql);
        refused_with(&error, state);
    };

    for key in [
        Table::create(Name::runtime("virtual_key"))
            .col(base())
            .col(derived(GeneratedKind::Virtual))
            .primary_key(Name::runtime("derived"))
            .to_string(),
        Table::create(Name::runtime("virtual_unique"))
            .col(base())
            .col(derived(GeneratedKind::Virtual))
            .unique(Name::runtime("derived"))
            .to_string(),
    ] {
        refused(key, &SqlState::FEATURE_NOT_SUPPORTED).await;
    }

    db.batch_execute(
        &Table::create(Name::runtime("stored_key"))
            .col(base())
            .col(derived(GeneratedKind::Stored))
            .primary_key(Name::runtime("derived"))
            .to_string(),
    )
    .await?;
    db.batch_execute(
        &Table::create(Name::runtime("virtual_plain"))
            .col(base())
            .col(derived(GeneratedKind::Virtual))
            .to_string(),
    )
    .await?;
    refused(
        Index::create(Name::runtime("virtual_plain"), Name::runtime("derived")).to_string(),
        &SqlState::FEATURE_NOT_SUPPORTED,
    )
    .await;

    db.batch_execute(
        &Type::create(Name::runtime("mood"))
            .values(["calm", "busy"])
            .to_string(),
    )
    .await?;
    refused(
        Table::create(Name::runtime("virtual_enum"))
            .col(ColumnDef::new(Name::runtime("label")).text().not_null())
            .col(
                ColumnDef::new_with_type(
                    Name::runtime("mood"),
                    ColumnType::Enum {
                        name: Name::runtime("mood"),
                        schema: None,
                        variants: Vec::new(),
                    },
                )
                .generated(
                    Expr::col(Name::runtime("label")).cast_as(Name::runtime("mood")),
                    GeneratedKind::Virtual,
                ),
            )
            .to_string(),
        &SqlState::FEATURE_NOT_SUPPORTED,
    )
    .await;

    for kind in [GeneratedKind::Stored, GeneratedKind::Virtual] {
        refused(
            Table::create(Name::runtime("volatile"))
                .col(base())
                .col(
                    ColumnDef::new(Name::runtime("noise"))
                        .double()
                        .generated(Func::random(), kind),
                )
                .to_string(),
            &SqlState::INVALID_OBJECT_DEFINITION,
        )
        .await;
        refused(
            Table::create(Name::runtime("nested"))
                .col(base())
                .col(derived(kind))
                .col(
                    ColumnDef::new(Name::runtime("again"))
                        .integer()
                        .generated(Expr::col(Name::runtime("derived")).add(1), kind),
                )
                .to_string(),
            &SqlState::INVALID_OBJECT_DEFINITION,
        )
        .await;
        refused(
            Table::create(Name::runtime("with_default"))
                .col(base())
                .col(
                    ColumnDef::new(Name::runtime("derived"))
                        .integer()
                        .default(1)
                        .generated(Expr::col(Name::runtime("base")), kind),
                )
                .to_string(),
            &SqlState::SYNTAX_ERROR,
        )
        .await;
    }

    drop(db);
    ctx.delete().await;
    Ok(())
}
