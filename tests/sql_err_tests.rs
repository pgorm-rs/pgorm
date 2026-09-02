#![allow(unused_imports, dead_code)]

pub mod common;
pub use common::{TestContext, bakery_chain::*, setup::*};
use pgorm::error::RuntimeErr;
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

    let cake = mud_cake.insert(db).await.expect("could not insert cake");

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

// [spec:pgorm:sem:error.model.sql-class+2]
// [spec:pgorm:sem:error.model.sql-class+2/test]    23505 -> Unique, 23503 -> ForeignKey
#[pgorm_macros::test]
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

    let cake = mud_cake.insert(&db).await.expect("could not insert cake");

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

// [spec:pgorm:sem:error.model.sql-class+2/test]    the SqlErr payload is the server DbError's message
#[pgorm_macros::test]
async fn sql_err_payload_carries_server_message() {
    let ctx = TestContext::new("bakery_chain_sql_err_payload_tests").await;
    create_tables(&ctx.db).await.unwrap();

    let db = ctx.db.get().await.unwrap();

    let cake = cake::ActiveModel {
        name: Set("Moldy Cake".to_owned()),
        price: Set(rust_dec(10.25)),
        gluten_free: Set(false),
        serial: Set(Uuid::new_v4()),
        bakery_id: Set(None),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert cake");

    let error = cake
        .into_active_model()
        .insert(&db)
        .await
        .expect_err("inserting should fail due to duplicate primary key");

    let DbErr::Postgres(driver) = &error else {
        panic!("expected DbErr::Postgres, got {error:?}");
    };
    let detail = driver.as_db_error().expect("a server-side error detail");
    assert_eq!(*detail.code(), SqlState::UNIQUE_VIOLATION);
    assert_eq!(
        error.sql_err(),
        Some(SqlErr::UniqueConstraintViolation(
            detail.message().to_owned()
        )),
        "the payload is the same DbError's message()"
    );

    drop(db);
    ctx.delete().await;
}

// [spec:pgorm:sem:error.model.sql-class+2/test]    every other error classifies as None
#[pgorm_macros::test]
async fn sql_err_none_for_unclassified_errors() {
    let ctx = TestContext::new("bakery_chain_sql_err_none_tests").await;
    let db = ctx.db.get().await.unwrap();

    // A different SQLSTATE: still `DbErr::Postgres`, still not a constraint violation.
    let undefined_table = db
        .query_one("SELECT id FROM absent_table", &[])
        .await
        .expect_err("there is no such table");
    assert_eq!(sql_state(&undefined_table), SqlState::UNDEFINED_TABLE);
    assert_eq!(undefined_table.sql_err(), None);

    // A `DbErr::Postgres` with no server-side `DbError`: the parameter cannot be
    // encoded, so the driver fails before the statement reaches the server.
    let client_side = db
        .query_one("SELECT $1::int4", &[&"not an int"])
        .await
        .expect_err("a string cannot be bound to an int4 parameter");
    let DbErr::Postgres(driver) = &client_side else {
        panic!("expected DbErr::Postgres, got {client_side:?}");
    };
    assert!(
        driver.as_db_error().is_none(),
        "this failure carries no server detail: {driver:?}"
    );
    assert_eq!(client_side.sql_err(), None);

    // Every non-`Postgres` variant is unclassifiable.
    for err in [
        DbErr::Custom("random error".to_owned()),
        DbErr::RecordNotFound,
        DbErr::RecordNotInserted,
        DbErr::Query(RuntimeErr::Internal("no socket".to_owned())),
        DbErr::Type("not a date".to_owned()),
    ] {
        assert_eq!(err.sql_err(), None, "{err:?}");
    }

    drop(db);
    ctx.delete().await;
}
