#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, features::*, setup::*};
use pgorm::TryInsertResult;
use pgorm::entity::prelude::*;
use pgorm::pgorm_query::OnConflict;
use pretty_assertions::assert_eq;

#[pgorm_macros::test]
async fn main() -> Result<(), Error> {
    let ctx = TestContext::new("upsert_tests").await;
    create_tables(&ctx.db).await?;

    let db = ctx.db.get().await?;
    create_insert_default(&db).await?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:exec.crud.insert+4/test]    the primary key read from the last
// RETURNING row of a batch, and RecordNotInserted when nothing is written
pub async fn create_insert_default(db: &DatabaseConnection) -> Result<(), Error> {
    use insert_default::*;

    let on_conflict = OnConflict::column(Column::Id).do_nothing();

    let res = Insert::many([
        ActiveModel { id: set(1) },
        ActiveModel { id: set(2) },
        ActiveModel { id: set(3) },
    ])
    .on_conflict(on_conflict.clone())
    .exec_returning_pk(db)
    .await;

    // [spec:pgorm:sem:exec.crud.insert+4] the key comes from the last
    // RETURNING row of the batch.
    assert_eq!(res?, 3);

    let res = Insert::many([
        ActiveModel { id: set(1) },
        ActiveModel { id: set(2) },
        ActiveModel { id: set(3) },
        ActiveModel { id: set(4) },
    ])
    .on_conflict(on_conflict.clone())
    .exec_returning_pk(db)
    .await;

    assert_eq!(res?, 4);

    let res = Insert::many([
        ActiveModel { id: set(1) },
        ActiveModel { id: set(2) },
        ActiveModel { id: set(3) },
        ActiveModel { id: set(4) },
    ])
    .on_conflict(on_conflict.clone())
    .exec_returning_pk(db)
    .await;

    assert!(matches!(res, Err(Error::RecordNotInserted)));

    let res = Insert::many([
        ActiveModel { id: set(1) },
        ActiveModel { id: set(2) },
        ActiveModel { id: set(3) },
        ActiveModel { id: set(4) },
    ])
    .on_conflict(on_conflict)
    .on_empty_do_nothing()
    .exec(db)
    .await;

    assert!(matches!(res, Ok(TryInsertResult::Conflicted)));

    Ok(())
}
