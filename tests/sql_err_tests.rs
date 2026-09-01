#![allow(unused_imports, dead_code)]

pub mod common;
pub use common::{TestContext, bakery_chain::*, setup::*};
pub use pgorm::{
    ActiveValue::Set, ConnectionTrait, DatabaseConnection, EntityName, entity::*, error::DbErr,
    error::SqlErr, tests_cfg,
};
use tokio_postgres::error::SqlState;
use uuid::Uuid;

fn sql_state(err: &DbErr) -> SqlState {
    match err {
        DbErr::Postgres(e) => e
            .code()
            .cloned()
            .unwrap_or_else(|| panic!("expected a SQLSTATE-carrying postgres error, got {e:?}")),
        other => panic!("expected DbErr::Postgres, got {other:?}"),
    }
}

#[pgorm_macros::test]
async fn main() {
    let ctx = TestContext::new("bakery_chain_sql_err_tests").await;
    create_tables(&ctx.db).await.unwrap();

    let db = ctx.db.get().await.unwrap();
    test_error(&db).await;

    drop(db);
    ctx.delete().await;
}

pub async fn test_error(db: &DatabaseConnection) {
    let mud_cake = cake::ActiveModel {
        name: Set("Moldy Cake".to_owned()),
        price: Set(rust_dec(10.25)),
        gluten_free: Set(false),
        serial: Set(Uuid::new_v4()),
        bakery_id: Set(None),
        ..Default::default()
    };

    let cake = mud_cake.save(db).await.expect("could not insert cake");

    let error: DbErr = cake
        .into_active_model()
        .insert(db)
        .await
        .expect_err("inserting should fail due to duplicate primary key");

    assert_eq!(sql_state(&error), SqlState::UNIQUE_VIOLATION);

    let fk_cake = cake::ActiveModel {
        name: Set("fk error Cake".to_owned()),
        price: Set(rust_dec(10.25)),
        gluten_free: Set(false),
        serial: Set(Uuid::new_v4()),
        bakery_id: Set(Some(1000)),
        ..Default::default()
    };

    let fk_error = fk_cake
        .insert(db)
        .await
        .expect_err("create foreign key should fail with non-primary key");

    assert_eq!(sql_state(&fk_error), SqlState::FOREIGN_KEY_VIOLATION);

    let invalid_error = DbErr::Custom("random error".to_string());
    assert_eq!(invalid_error.sql_err(), None);
}

// [spec:pgorm:sem:error.model.sql-class+1]
#[pgorm_macros::test]
#[ignore = "sql_err classifier not implemented for tokio-postgres"]
async fn sql_err_classification() {
    let ctx = TestContext::new("bakery_chain_sql_err_classification_tests").await;
    create_tables(&ctx.db).await.unwrap();

    let db = ctx.db.get().await.unwrap();

    let mud_cake = cake::ActiveModel {
        name: Set("Moldy Cake".to_owned()),
        price: Set(rust_dec(10.25)),
        gluten_free: Set(false),
        serial: Set(Uuid::new_v4()),
        bakery_id: Set(None),
        ..Default::default()
    };

    let cake = mud_cake.save(&db).await.expect("could not insert cake");

    let error: DbErr = cake
        .into_active_model()
        .insert(&db)
        .await
        .expect_err("inserting should fail due to duplicate primary key");

    assert!(matches!(
        error.sql_err(),
        Some(SqlErr::UniqueConstraintViolation(_))
    ));

    let fk_cake = cake::ActiveModel {
        name: Set("fk error Cake".to_owned()),
        price: Set(rust_dec(10.25)),
        gluten_free: Set(false),
        serial: Set(Uuid::new_v4()),
        bakery_id: Set(Some(1000)),
        ..Default::default()
    };

    let fk_error = fk_cake
        .insert(&db)
        .await
        .expect_err("create foreign key should fail with non-primary key");

    assert!(matches!(
        fk_error.sql_err(),
        Some(SqlErr::ForeignKeyConstraintViolation(_))
    ));

    drop(db);
    ctx.delete().await;
}
