#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, bakery_chain::*, setup::*};
pub use pgorm::entity::*;
pub use pgorm::{ActiveValue::Set, ConnectionTrait, DbErr, QueryFilter, QuerySelect};

// Run the test locally:
// DATABASE_URL=postgres://postgres:postgres@127.0.0.1:54329 cargo test --test query_tests
#[pgorm_macros::test]
pub async fn find_one_with_no_result() {
    let ctx = TestContext::new("find_one_with_no_result").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();

    let bakery = Bakery::find().one_opt(&db).await.unwrap();
    assert_eq!(bakery, None);

    assert!(matches!(
        Bakery::find().one(&db).await,
        Err(DbErr::RecordNotFound)
    ));

    drop(db);
    ctx.delete().await;
}

#[pgorm_macros::test]
pub async fn find_one_with_result() {
    let ctx = TestContext::new("find_one_with_result").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();

    let bakery = bakery::ActiveModel {
        name: Set("SeaSide Bakery".to_owned()),
        profit_margin: Set(10.4),
        ..Default::default()
    }
    .save(&db)
    .await
    .expect("could not insert bakery");

    let result = Bakery::find().one(&db).await.unwrap();

    assert_eq!(result.id, bakery.id.unwrap());

    drop(db);
    ctx.delete().await;
}

#[pgorm_macros::test]
pub async fn find_by_id_with_no_result() {
    let ctx = TestContext::new("find_by_id_with_no_result").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();

    let bakery = Bakery::find_by_id(999).one_opt(&db).await.unwrap();
    assert_eq!(bakery, None);

    assert!(matches!(
        Bakery::find_by_id(999).one(&db).await,
        Err(DbErr::RecordNotFound)
    ));

    drop(db);
    ctx.delete().await;
}

#[pgorm_macros::test]
pub async fn find_by_id_with_result() {
    let ctx = TestContext::new("find_by_id_with_result").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();

    let bakery = bakery::ActiveModel {
        name: Set("SeaSide Bakery".to_owned()),
        profit_margin: Set(10.4),
        ..Default::default()
    }
    .save(&db)
    .await
    .expect("could not insert bakery");

    let result = Bakery::find_by_id(bakery.id.clone().unwrap())
        .one(&db)
        .await
        .unwrap();

    assert_eq!(result.id, bakery.id.unwrap());

    drop(db);
    ctx.delete().await;
}

#[pgorm_macros::test]
pub async fn find_all_with_no_result() {
    let ctx = TestContext::new("find_all_with_no_result").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();

    let bakeries = Bakery::find().all(&db).await.unwrap();
    assert_eq!(bakeries.len(), 0);

    drop(db);
    ctx.delete().await;
}

#[pgorm_macros::test]
pub async fn find_all_with_result() {
    let ctx = TestContext::new("find_all_with_result").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();

    let _ = bakery::ActiveModel {
        name: Set("SeaSide Bakery".to_owned()),
        profit_margin: Set(10.4),
        ..Default::default()
    }
    .save(&db)
    .await
    .expect("could not insert bakery");

    let _ = bakery::ActiveModel {
        name: Set("Top Bakery".to_owned()),
        profit_margin: Set(15.0),
        ..Default::default()
    }
    .save(&db)
    .await
    .expect("could not insert bakery");

    let bakeries = Bakery::find().all(&db).await.unwrap();

    assert_eq!(bakeries.len(), 2);

    drop(db);
    ctx.delete().await;
}

#[pgorm_macros::test]
pub async fn find_all_filter_no_result() {
    let ctx = TestContext::new("find_all_filter_no_result").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();

    let _ = bakery::ActiveModel {
        name: Set("SeaSide Bakery".to_owned()),
        profit_margin: Set(10.4),
        ..Default::default()
    }
    .save(&db)
    .await
    .expect("could not insert bakery");

    let _ = bakery::ActiveModel {
        name: Set("Top Bakery".to_owned()),
        profit_margin: Set(15.0),
        ..Default::default()
    }
    .save(&db)
    .await
    .expect("could not insert bakery");

    let bakeries = Bakery::find()
        .filter(bakery::Column::Name.contains("Good"))
        .all(&db)
        .await
        .unwrap();

    assert_eq!(bakeries.len(), 0);

    drop(db);
    ctx.delete().await;
}

#[pgorm_macros::test]
pub async fn find_all_filter_with_results() {
    let ctx = TestContext::new("find_all_filter_with_results").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();

    let _ = bakery::ActiveModel {
        name: Set("SeaSide Bakery".to_owned()),
        profit_margin: Set(10.4),
        ..Default::default()
    }
    .save(&db)
    .await
    .expect("could not insert bakery");

    let _ = bakery::ActiveModel {
        name: Set("Top Bakery".to_owned()),
        profit_margin: Set(15.0),
        ..Default::default()
    }
    .save(&db)
    .await
    .expect("could not insert bakery");

    let bakeries = Bakery::find()
        .filter(bakery::Column::Name.contains("Bakery"))
        .all(&db)
        .await
        .unwrap();

    assert_eq!(bakeries.len(), 2);

    drop(db);
    ctx.delete().await;
}

#[pgorm_macros::test]
pub async fn select_only_exclude_option_fields() {
    let ctx = TestContext::new("select_only_exclude_option_fields").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();

    let _ = customer::ActiveModel {
        name: Set("Alice".to_owned()),
        notes: Set(Some("Want to communicate with Bob".to_owned())),
        ..Default::default()
    }
    .save(&db)
    .await
    .expect("could not insert customer");

    let _ = customer::ActiveModel {
        name: Set("Bob".to_owned()),
        notes: Set(Some("Just listening".to_owned())),
        ..Default::default()
    }
    .save(&db)
    .await
    .expect("could not insert customer");

    // An absent column is not a NULL value: only `WasNull` decodes to `None`,
    // every other decode error propagates.
    let err = Customer::find()
        .select_only()
        .column(customer::Column::Id)
        .column(customer::Column::Name)
        .all(&db)
        .await
        .expect_err("an absent `notes` column must not decode as None");

    assert!(matches!(err, DbErr::Postgres(_)), "unexpected error: {err}");

    let customers = Customer::find()
        .select_only()
        .column(customer::Column::Id)
        .column(customer::Column::Name)
        .column(customer::Column::Notes)
        .all(&db)
        .await
        .unwrap();

    assert_eq!(customers.len(), 2);
    assert_eq!(
        customers[0].notes,
        Some("Want to communicate with Bob".to_owned())
    );
    assert_eq!(customers[1].notes, Some("Just listening".to_owned()));

    drop(db);
    ctx.delete().await;
}
