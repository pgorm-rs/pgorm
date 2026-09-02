#![allow(unused_imports, dead_code)]

pub mod common;
mod crud;

pub use common::{TestContext, bakery_chain::*, setup::*};
pub use pgorm::{
    ActiveValue::Set, DatabaseConnection, DatabasePool, EntityName, entity::*, error::DbErr,
    tests_cfg,
};

pub use crud::*;
use pgorm::TryInsertResult;

#[pgorm_macros::test]
async fn main() {
    let ctx = TestContext::new("bakery_chain_empty_insert_tests").await;
    create_tables(&ctx.db).await.unwrap();

    let db = ctx.db.get().await.unwrap();
    test(&db).await;
    columns_mismatch_is_refused(&db).await;

    drop(db);
    ctx.delete().await;
}

// [spec:pgorm:sem:exec.crud.try-insert+1/test]    `TryInsert::exec` reporting
// Inserted, Conflicted and Empty
// [spec:pgorm:sem:query.build.insert.empty-failsafe+1/test]    `on_empty_do_nothing`
// / `on_conflict_do_nothing` produce a `TryInsert` whose `exec` maps
// RecordNotInserted to Conflicted and an empty batch to Empty
pub async fn test(db: &DatabaseConnection) {
    let seaside_bakery = bakery::ActiveModel {
        name: Set("SeaSide Bakery".to_owned()),
        profit_margin: Set(10.4),
        ..Default::default()
    };

    let res = Bakery::insert(seaside_bakery)
        .on_empty_do_nothing()
        .exec(db)
        .await;

    assert!(matches!(res, Ok(TryInsertResult::Inserted(_))));

    let double_seaside_bakery = bakery::ActiveModel {
        name: Set("SeaSide Bakery".to_owned()),
        profit_margin: Set(10.4),
        id: Set(1),
    };

    let conflict_insert = Bakery::insert_many([double_seaside_bakery])
        .on_conflict_do_nothing()
        .exec(db)
        .await;

    assert!(matches!(conflict_insert, Ok(TryInsertResult::Conflicted)));

    // [spec:pgorm:sem:query.build.insert.empty-failsafe+1] An empty batch is a
    // no-op that reports Empty before any SQL is issued.
    let empty_insert = Bakery::insert_many(std::iter::empty::<bakery::ActiveModel>())
        .on_empty_do_nothing()
        .exec(db)
        .await;

    assert!(matches!(empty_insert, Ok(TryInsertResult::Empty)));

    // A model with every column NotSet records no column either, so it reaches
    // the same empty state and leaves the database untouched.
    let blank_insert = Bakery::insert(bakery::ActiveModel {
        ..Default::default()
    })
    .on_empty_do_nothing()
    .exec(db)
    .await;

    assert!(matches!(blank_insert, Ok(TryInsertResult::Empty)));
    assert_eq!(Bakery::find().all(db).await.unwrap().len(), 1);
}

// [spec:pgorm:req:query.build.insert.uniform-columns+1/test]    every execution
// path of both insert types reports the recorded mismatch instead of sending
// SQL, so the batch leaves the database untouched
pub async fn columns_mismatch_is_refused(db: &DatabaseConnection) {
    let mismatched = || {
        Bakery::insert_many([
            bakery::ActiveModel {
                name: Set("Hillside Bakery".to_owned()),
                profit_margin: Set(1.0),
                ..Default::default()
            },
            bakery::ActiveModel {
                id: Set(9),
                name: Set("Riverside Bakery".to_owned()),
                profit_margin: Set(2.0),
            },
        ])
    };
    let expected = "Query Error: models added to one insert do not share a column set: \
                    `id` is set in a later model but not in the first";

    let refused = [
        mismatched().exec(db).await.err(),
        mismatched().exec_without_returning(db).await.err(),
        mismatched().exec_with_returning(db).await.err(),
        mismatched().do_nothing().exec(db).await.err(),
        mismatched()
            .do_nothing()
            .exec_without_returning(db)
            .await
            .err(),
        mismatched()
            .do_nothing()
            .exec_with_returning(db)
            .await
            .err(),
    ];

    for err in refused {
        assert_eq!(
            err.expect("a mismatched batch cannot execute").to_string(),
            expected
        );
    }

    assert_eq!(Bakery::find().all(db).await.unwrap().len(), 1);
}
