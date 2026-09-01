#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, features::*, setup::*};
use pgorm::{ConnectionTrait, DatabaseConnection, entity::prelude::*};
use pretty_assertions::assert_eq;

const NO_PARAMS: [&(dyn tokio_postgres::types::ToSql + Sync); 0] = [];

#[pgorm_macros::test]
async fn main() -> Result<(), DbErr> {
    let ctx = TestContext::new("execute_unprepared_tests").await;
    create_tables(&ctx.db).await?;
    let db = ctx.db.get().await?;

    execute_raw_statements(&db).await?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

pub async fn execute_raw_statements(db: &DatabaseConnection) -> Result<(), DbErr> {
    use insert_default::*;

    db.execute_raw(
        "INSERT INTO insert_default (id) VALUES (1), (2), (3), (4), (5)",
        NO_PARAMS,
    )
    .await?;

    db.execute_raw("DELETE FROM insert_default WHERE id % 2 = 0", NO_PARAMS)
        .await?;

    assert_eq!(
        Entity::find().all(db).await?,
        [Model { id: 1 }, Model { id: 3 }, Model { id: 5 }]
    );

    Ok(())
}
