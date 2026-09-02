pub use super::*;
use pgorm::error::*;
use uuid::Uuid;

pub async fn test_cake_error(db: &DatabaseConnection) {
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

    match &error {
        DbErr::Postgres(e) => {
            let db_error = e.as_db_error().expect("expected a database error");
            assert_eq!(db_error.code().code(), "23505");
        }
        _ => panic!("Unexpected Error kind: {error:?}"),
    }
}
