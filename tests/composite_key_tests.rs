#![allow(unused_imports, dead_code)]

//! Composite primary keys against a live PostgreSQL server: the multi-tenant
//! shape whose second column the database generates.

pub mod common;

pub use common::{TestContext, setup::*};
use pgorm::{ConnectionTrait, Error, QueryTrait, Schema, entity::prelude::*};
use pretty_assertions::assert_eq;
use tokio_postgres::error::SqlState;

/// The tenant is supplied, the row number generated: `id` is an identity
/// inside the key, and nothing else about the table is unusual.
mod ticket {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "ticket")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub tenant_id: i32,
        #[pgorm(primary_key, identity)]
        pub id: i64,
        pub title: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// The same key with the generated part `BY DEFAULT`: a supplied number is
/// kept, an omitted one drawn from the sequence.
mod imported_ticket {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "imported_ticket")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub tenant_id: i32,
        #[pgorm(primary_key, identity_by_default)]
        pub id: i32,
        pub title: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

async fn create_from_entity<C, E>(db: &C, entity: E) -> Result<(), Error>
where
    C: ConnectionTrait,
    E: EntityTrait,
{
    let stmt = Schema::new().create_table_from_entity(entity);
    db.execute(&stmt.to_string(), &[]).await?;
    Ok(())
}

fn refused_with(error: &Error, state: &SqlState) {
    match error {
        Error::Postgres(e) => assert_eq!(e.code(), Some(state), "{e}"),
        other => panic!("expected Error::Postgres, got {other:?}"),
    }
}

fn new_ticket(tenant_id: i32, title: &str) -> ticket::ActiveModel {
    ticket::ActiveModel {
        tenant_id: set(tenant_id),
        title: set(title),
        ..Default::default()
    }
}

// [spec:pgorm:sem:schema.from-entity+5/test]    the generated part of a
// composite key gets its identity in the table schema-gen builds, and nothing
// else in the key does
// [spec:pgorm:sem:exec.crud.insert+5/test]    an insert leaving the generated
// part NotSet names no value for it, and the key it reports is the whole tuple
// the database wrote, generated part included
#[pgorm_macros::test]
async fn generated_key_part_comes_back_in_the_tuple() -> Result<(), Error> {
    let ctx = TestContext::new("composite_key_generated_part").await;
    let db = ctx.db.get().await?;
    create_from_entity(&db, ticket::Entity).await?;

    let insert = Insert::one(new_ticket(7, "first"));
    assert_eq!(
        insert.as_query().to_string(),
        r#"INSERT INTO "ticket" ("tenant_id", "title") VALUES (7, 'first')"#,
        "the generated column is not named"
    );
    assert_eq!(insert.exec_returning_pk(&db).await?, (7, 1));
    assert_eq!(
        Insert::one(new_ticket(8, "second"))
            .exec_returning_pk(&db)
            .await?,
        (8, 2)
    );

    let third = Insert::one(new_ticket(7, "third"))
        .exec_returning_model(&db)
        .await?;
    assert_eq!(
        third,
        ticket::Model {
            tenant_id: 7,
            id: 3,
            title: "third".to_owned(),
        }
    );
    let fourth = new_ticket(8, "fourth").insert(&db).await?;
    assert_eq!((fourth.tenant_id, fourth.id), (8, 4));

    assert_eq!(
        ticket::Entity::find_by_id((7, 3)).one(&db).await?.title,
        "third"
    );

    drop(db);
    ctx.delete().await;
    Ok(())
}

// [spec:pgorm:sem:exec.crud.insert+5/test]    a many-row insert leaving the
// generated part NotSet writes every row with its own generated number, and
// reports the tuple of the last
#[pgorm_macros::test]
async fn many_rows_each_get_a_generated_key_part() -> Result<(), Error> {
    let ctx = TestContext::new("composite_key_generated_many").await;
    let db = ctx.db.get().await?;
    create_from_entity(&db, ticket::Entity).await?;

    let last = Insert::many([new_ticket(1, "a"), new_ticket(2, "b"), new_ticket(1, "c")])
        .exec_returning_pk(&db)
        .await?;
    assert_eq!(last, (1, 3));

    let written = Insert::many([new_ticket(3, "d"), new_ticket(3, "e")])
        .exec(&db)
        .await?;
    assert_eq!(written, 2);

    let keys: Vec<(i32, i64)> = ticket::Entity::find()
        .order_by_asc(ticket::Column::Id)
        .all(&db)
        .await?
        .into_iter()
        .map(|row| (row.tenant_id, row.id))
        .collect();
    assert_eq!(keys, [(1, 1), (2, 2), (1, 3), (3, 4), (3, 5)]);

    drop(db);
    ctx.delete().await;
    Ok(())
}

// [spec:pgorm:sem:macros.derive.entity-model.primary-key+3/test]    `identity`
// is `GENERATED ALWAYS`: a number the insert supplies is refused by the server
// (428C9), so the generated part cannot be claimed by accident;
// `identity_by_default` keeps a supplied number and generates an omitted one
#[pgorm_macros::test]
async fn identity_forms_answer_a_supplied_key_part() -> Result<(), Error> {
    let ctx = TestContext::new("composite_key_identity_forms").await;
    let db = ctx.db.get().await?;
    create_from_entity(&db, ticket::Entity).await?;
    create_from_entity(&db, imported_ticket::Entity).await?;

    let claimed = Insert::one(ticket::ActiveModel {
        tenant_id: set(1),
        id: set(42),
        title: set("claimed"),
    })
    .exec_returning_pk(&db)
    .await;
    refused_with(
        &claimed.expect_err("ALWAYS refuses a supplied value"),
        &SqlState::GENERATED_ALWAYS,
    );

    let kept = Insert::one(imported_ticket::ActiveModel {
        tenant_id: set(1),
        id: set(42),
        title: set("kept"),
    })
    .exec_returning_pk(&db)
    .await?;
    assert_eq!(kept, (1, 42));
    let drawn = Insert::one(imported_ticket::ActiveModel {
        tenant_id: set(1),
        title: set("drawn"),
        ..Default::default()
    })
    .exec_returning_pk(&db)
    .await?;
    assert_eq!(drawn, (1, 1));

    drop(db);
    ctx.delete().await;
    Ok(())
}
