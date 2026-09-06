//! A related row that is present but undecodable must be reported, not
//! silently turned into "no related row".
//!
//! Every joined decode funnels through
//! [`FromQueryResult::from_query_result_optional`], so the tables here are
//! built with raw DDL: the point is a column whose PostgreSQL type the Rust
//! model cannot decode, which no entity-derived schema would produce.

#![allow(unused_imports, dead_code)]

pub mod common;

use futures::StreamExt;
use pgorm::tests_cfg::{cake, entity_linked, filling, fruit, vendor};
use pgorm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, Error, FromQueryResult,
    QueryFilter, QuerySelect, alias,
};
use pretty_assertions::assert_eq;

pub use common::TestContext;

/// The cake that has a related row, whose related row does not decode.
const BROKEN: i32 = 1;
/// The cake the outer join matches nothing for.
const LONELY: i32 = 2;

fn cake_broken() -> cake::Model {
    cake::Model {
        id: BROKEN,
        name: "Cheesecake".to_owned(),
    }
}

fn cake_lonely() -> cake::Model {
    cake::Model {
        id: LONELY,
        name: "Lonely".to_owned(),
    }
}

/// A decode failure the caller can act on, rather than a missing row.
#[track_caller]
fn assert_wrong_type<T: std::fmt::Debug>(result: Result<T, Error>) {
    let err = match result {
        Ok(ok) => panic!("a present but undecodable related row was reported as {ok:?}"),
        Err(err) => err,
    };
    let rendered = err.to_string();
    assert!(
        rendered.contains("WrongType"),
        "expected a decode failure naming the type mismatch, got: {rendered}"
    );
}

// ---------------------------------------------------------------------------
// find_also_related / find_with_related / streams
// ---------------------------------------------------------------------------

async fn related_schema(db: &DatabaseConnection) -> Result<(), Error> {
    db.batch_execute(
        r#"
        CREATE TABLE "cake" ("id" int PRIMARY KEY, "name" text NOT NULL);
        CREATE TABLE "fruit" ("id" int PRIMARY KEY, "name" int NOT NULL, "cake_id" int);
        INSERT INTO "cake" VALUES (1, 'Cheesecake'), (2, 'Lonely');
        INSERT INTO "fruit" VALUES (10, 123, 1);
        "#,
    )
    .await
}

// [spec:pgorm:req:exec.decode.absent/test]    a present related row whose
// columns do not decode is an error on every joined path, while an outer join
// that matched nothing stays `None`
#[pgorm_macros::test]
async fn related_decode_errors() -> Result<(), Error> {
    let ctx = TestContext::new("joined_decode_related").await;
    let db = ctx.db.get().await?;
    related_schema(&db).await?;

    // The related side on its own already reports the mismatch, and the join
    // must not be the weaker path.
    assert_wrong_type(fruit::Entity::find().all(&db).await);

    // find_also_related: present-but-undecodable
    assert_wrong_type(
        cake::Entity::find()
            .filter(cake::Column::Id.eq(BROKEN))
            .find_also_related(fruit::Entity)
            .all(&db)
            .await,
    );
    assert_wrong_type(
        cake::Entity::find()
            .filter(cake::Column::Id.eq(BROKEN))
            .find_also_related(fruit::Entity)
            .one(&db)
            .await,
    );
    assert_wrong_type(
        cake::Entity::find()
            .filter(cake::Column::Id.eq(BROKEN))
            .find_also_related(fruit::Entity)
            .one_opt(&db)
            .await,
    );

    // find_also_related: genuinely absent
    assert_eq!(
        cake::Entity::find()
            .filter(cake::Column::Id.eq(LONELY))
            .find_also_related(fruit::Entity)
            .all(&db)
            .await?,
        [(cake_lonely(), None)]
    );

    // find_with_related, through the regroup path
    assert_wrong_type(
        cake::Entity::find()
            .filter(cake::Column::Id.eq(BROKEN))
            .find_with_related(fruit::Entity)
            .all(&db)
            .await,
    );
    assert_eq!(
        cake::Entity::find()
            .filter(cake::Column::Id.eq(LONELY))
            .find_with_related(fruit::Entity)
            .all(&db)
            .await?,
        [(cake_lonely(), vec![])]
    );

    // The stream decodes row by row through the same selector.
    let mut stream = cake::Entity::find()
        .filter(cake::Column::Id.eq(BROKEN))
        .find_also_related(fruit::Entity)
        .stream(&db)
        .await?;
    assert_wrong_type(stream.next().await.expect("the join produced one row"));
    drop(stream);

    let mut stream = cake::Entity::find()
        .filter(cake::Column::Id.eq(LONELY))
        .find_also_related(fruit::Entity)
        .stream(&db)
        .await?;
    assert_eq!(
        stream.next().await.expect("the join produced one row")?,
        (cake_lonely(), None)
    );
    drop(stream);

    drop(db);
    ctx.delete().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Custom projections over the joined pair
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, FromQueryResult)]
struct CakeIdName {
    id: i32,
    name: String,
}

#[derive(Debug, PartialEq, FromQueryResult)]
struct FruitIdName {
    id: i32,
    name: String,
}

// [spec:pgorm:req:exec.decode.absent/test]    a custom projection over the
// joined pair follows the same rule, and a column the projection never
// produced is reported rather than read as an absent row
#[pgorm_macros::test]
async fn projected_decode_errors() -> Result<(), Error> {
    let ctx = TestContext::new("joined_decode_projected").await;
    let db = ctx.db.get().await?;
    related_schema(&db).await?;

    let projected = |cake_id: i32| {
        cake::Entity::find()
            .filter(cake::Column::Id.eq(cake_id))
            .find_also_related(fruit::Entity)
            .select_only()
            .column_as(cake::Column::Id, alias("A_id"))
            .column_as(cake::Column::Name, alias("A_name"))
            .column_as(fruit::Column::Id, alias("B_id"))
            .column_as(fruit::Column::Name, alias("B_name"))
            .into_model::<CakeIdName, FruitIdName>()
    };

    assert_wrong_type(projected(BROKEN).all(&db).await);
    assert_eq!(
        projected(LONELY).all(&db).await?,
        [(
            CakeIdName {
                id: LONELY,
                name: "Lonely".to_owned(),
            },
            None
        )]
    );

    // `B_name` is never projected, so `FruitIdName` names a column the result
    // set does not carry. That is a projection mistake, not an absent row, on
    // the matched row and the unmatched one alike.
    let under_projected = |cake_id: i32| {
        cake::Entity::find()
            .filter(cake::Column::Id.eq(cake_id))
            .find_also_related(fruit::Entity)
            .select_only()
            .column_as(cake::Column::Id, alias("A_id"))
            .column_as(cake::Column::Name, alias("A_name"))
            .column_as(fruit::Column::Id, alias("B_id"))
            .into_model::<CakeIdName, FruitIdName>()
    };

    let matched = under_projected(BROKEN)
        .all(&db)
        .await
        .expect_err("a column the projection omits must be reported");
    assert!(
        matched.to_string().contains("B_name"),
        "expected the missing column to be named, got: {matched}"
    );

    // The unmatched row cannot be called absent either: `B_id` is `NULL` but
    // `B_name` is not a column of the result set at all, so the witness is
    // incomplete and the decode failure stands.
    under_projected(LONELY)
        .all(&db)
        .await
        .expect_err("an incomplete witness must not be read as an absent row");

    drop(db);
    ctx.delete().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// find_also_linked / find_with_linked
// ---------------------------------------------------------------------------

// [spec:pgorm:req:exec.decode.absent/test]    the multi-hop linked join reads
// its far side through the same optional decode
#[pgorm_macros::test]
async fn linked_decode_errors() -> Result<(), Error> {
    let ctx = TestContext::new("joined_decode_linked").await;
    let db = ctx.db.get().await?;
    db.batch_execute(
        r#"
        CREATE TABLE "cake" ("id" int PRIMARY KEY, "name" text NOT NULL);
        CREATE TABLE "filling" ("id" int PRIMARY KEY, "name" text NOT NULL, "vendor_id" int);
        CREATE TABLE "vendor" ("id" int PRIMARY KEY, "name" int NOT NULL);
        CREATE TABLE "cake_filling" (
            "cake_id" int, "filling_id" int, PRIMARY KEY ("cake_id", "filling_id")
        );
        INSERT INTO "cake" VALUES (1, 'Cheesecake'), (2, 'Lonely');
        INSERT INTO "vendor" VALUES (7, 123);
        INSERT INTO "filling" VALUES (5, 'Cream', 7);
        INSERT INTO "cake_filling" VALUES (1, 5);
        "#,
    )
    .await?;

    assert_wrong_type(
        cake::Entity::find()
            .filter(cake::Column::Id.eq(BROKEN))
            .find_also_linked(entity_linked::CakeToFillingVendor)
            .all(&db)
            .await,
    );
    assert_eq!(
        cake::Entity::find()
            .filter(cake::Column::Id.eq(LONELY))
            .find_also_linked(entity_linked::CakeToFillingVendor)
            .all(&db)
            .await?,
        [(cake_lonely(), None)]
    );

    assert_wrong_type(
        cake::Entity::find()
            .filter(cake::Column::Id.eq(BROKEN))
            .find_with_linked(entity_linked::CakeToFillingVendor)
            .all(&db)
            .await,
    );
    assert_eq!(
        cake::Entity::find()
            .filter(cake::Column::Id.eq(LONELY))
            .find_with_linked(entity_linked::CakeToFillingVendor)
            .all(&db)
            .await?,
        [(cake_lonely(), vec![])]
    );

    drop(db);
    ctx.delete().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Payload decodes: an enum label and a JSON body the model cannot read
// ---------------------------------------------------------------------------

mod tea {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
    #[pgorm(rs_type = "String", db_type = "Enum", enum_name = "tea")]
    pub enum Tea {
        #[pgorm(string_value = "EverydayTea")]
        EverydayTea,
    }

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "brew")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        pub cake_id: Option<i32>,
        pub tea: Tea,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[pgorm(
            belongs_to = "pgorm::tests_cfg::cake::Entity",
            from = "Column::CakeId",
            to = "pgorm::tests_cfg::cake::Column::Id"
        )]
        Cake,
    }

    impl Related<pgorm::tests_cfg::cake::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Cake.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

impl pgorm::Related<tea::Entity> for cake::Entity {
    fn to() -> pgorm::RelationDef {
        use pgorm::RelationTrait;

        tea::Relation::Cake.def().rev()
    }
}

// [spec:pgorm:req:exec.decode.absent/test]    an unrecognised enum label in a
// present related row is reported, not read as an absent row
#[pgorm_macros::test]
async fn enum_payload_decode_errors() -> Result<(), Error> {
    use pgorm::RelationTrait;

    let ctx = TestContext::new("joined_decode_enum").await;
    let db = ctx.db.get().await?;
    db.batch_execute(
        r#"
        CREATE TABLE "cake" ("id" int PRIMARY KEY, "name" text NOT NULL);
        CREATE TYPE "tea" AS ENUM ('EverydayTea', 'BreakfastTea');
        CREATE TABLE "brew" (
            "id" int PRIMARY KEY, "cake_id" int, "tea" "tea" NOT NULL
        );
        INSERT INTO "cake" VALUES (1, 'Cheesecake'), (2, 'Lonely');
        INSERT INTO "brew" VALUES (10, 1, 'BreakfastTea');
        "#,
    )
    .await?;

    // `BreakfastTea` is a label the Rust enum does not carry, so the related
    // row is present and undecodable.
    let err = cake::Entity::find()
        .filter(cake::Column::Id.eq(BROKEN))
        .find_also_related(tea::Entity)
        .all(&db)
        .await
        .expect_err("an unknown enum label must be reported");
    assert!(
        err.to_string().contains("BreakfastTea"),
        "expected the unknown label to be named, got: {err}"
    );

    assert_eq!(
        cake::Entity::find()
            .filter(cake::Column::Id.eq(LONELY))
            .find_also_related(tea::Entity)
            .all(&db)
            .await?,
        [(cake_lonely(), None)]
    );

    drop(db);
    ctx.delete().await;
    Ok(())
}
