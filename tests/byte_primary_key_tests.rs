#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, features::*, setup::*};
use pgorm::{
    ActiveValue::{Set, Unchanged},
    DatabaseConnection,
    entity::prelude::*,
    entity::*,
};
use pretty_assertions::assert_eq;

#[pgorm_macros::test]
async fn main() -> Result<(), DbErr> {
    let ctx = TestContext::new("byte_primary_key_tests").await;
    create_tables(&ctx.db).await?;

    let db = ctx.db.get().await?;
    create_and_update(&db).await?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:def:exec.crud/test]    `into_values` decoding through
// `SelectGetableValue` and `into_tuple` through `SelectGetableTuple`
// [spec:pgorm:sem:exec.crud.update+3/test]    `UpdateOne::exec` returns the
// updated model, and surfaces RecordNotFound when the filter matches nothing
pub async fn create_and_update(db: &DatabaseConnection) -> Result<(), DbErr> {
    use common::features::byte_primary_key::*;

    let model = Model {
        id: vec![1, 2, 3],
        value: "First Row".to_owned(),
    };

    let res = Entity::insert(model.clone().into_active_model())
        .exec(db)
        .await?;

    assert_eq!(Entity::find().one_opt(db).await?, Some(model.clone()));

    assert_eq!(res.last_insert_id, model.id);

    let updated_active_model = ActiveModel {
        value: Set("First Row (Updated)".to_owned()),
        ..model.clone().into_active_model()
    };

    let update_res = Entity::update(updated_active_model.clone())?
        .filter(Column::Id.eq(vec![1_u8, 2_u8, 4_u8])) // annotate it as Vec<u8> explicitly
        .exec(db)
        .await;

    // [spec:pgorm:sem:exec.crud.update+3] UpdateOne decodes through `one`, so a
    // filter matching zero rows surfaces RecordNotFound.
    assert_eq!(update_res, Err(DbErr::RecordNotFound));

    let update_res = Entity::update(updated_active_model)?
        .filter(Column::Id.eq(vec![1_u8, 2_u8, 3_u8])) // annotate it as Vec<u8> explicitly
        .exec(db)
        .await?;

    assert_eq!(
        update_res,
        Model {
            id: vec![1, 2, 3],
            value: "First Row (Updated)".to_owned(),
        }
    );

    assert_eq!(
        Entity::find()
            .filter(Column::Id.eq(vec![1_u8, 2_u8, 3_u8])) // annotate it as Vec<u8> explicitly
            .one_opt(db)
            .await?,
        Some(Model {
            id: vec![1, 2, 3],
            value: "First Row (Updated)".to_owned(),
        })
    );

    assert_eq!(
        Entity::find()
            .filter(Column::Id.eq(vec![1_u8, 2_u8, 3_u8])) // annotate it as Vec<u8> explicitly
            .into_values::<_, Column>()
            .one_opt(db)
            .await?,
        Some((vec![1_u8, 2_u8, 3_u8], "First Row (Updated)".to_owned(),))
    );

    assert_eq!(
        Entity::find()
            .filter(Column::Id.eq(vec![1_u8, 2_u8, 3_u8])) // annotate it as Vec<u8> explicitly
            .into_tuple()
            .one_opt(db)
            .await?,
        Some((vec![1_u8, 2_u8, 3_u8], "First Row (Updated)".to_owned(),))
    );

    Ok(())
}
