#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, features::*, setup::*};
use pgorm::{DatabaseConnection, entity::prelude::*};
use pretty_assertions::assert_eq;

#[pgorm_macros::test]
async fn main() -> Result<(), DbErr> {
    let ctx = TestContext::new("insert_default_tests").await;
    create_tables(&ctx.db).await?;

    let db = ctx.db.get().await?;
    create_insert_default(&db).await?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

pub async fn create_insert_default(db: &DatabaseConnection) -> Result<(), DbErr> {
    use insert_default::*;

    let active_model = ActiveModel {
        ..Default::default()
    };

    active_model.clone().insert(db).await?;
    active_model.clone().insert(db).await?;
    active_model.insert(db).await?;

    assert_eq!(
        Entity::find().all(db).await?,
        [Model { id: 1 }, Model { id: 2 }, Model { id: 3 }]
    );

    Ok(())
}
