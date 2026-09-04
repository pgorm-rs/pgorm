pub use super::*;
use pgorm::set;
use uuid::Uuid;

pub async fn test_delete_cake(db: &DatabaseConnection) {
    let initial_cakes = Cake::find().all(db).await.unwrap().len();

    let seaside_bakery = bakery::ActiveModel {
        name: set("SeaSide Bakery"),
        profit_margin: set(10.4),
        ..Default::default()
    };
    let bakery_insert_res = Insert::one(seaside_bakery)
        .exec_returning_pk(db)
        .await
        .expect("could not insert bakery");

    let mud_cake = cake::ActiveModel {
        name: set("Mud Cake"),
        price: set(rust_dec(10.25)),
        gluten_free: set(false),
        serial: set(Uuid::new_v4()),
        bakery_id: set(Some(bakery_insert_res)),
        ..Default::default()
    };

    let cake = mud_cake.insert(db).await.expect("could not insert cake");

    let cakes = Cake::find().all(db).await.unwrap();
    assert_eq!(cakes.len(), initial_cakes + 1);

    let _result = cake.delete(db).await.expect("failed to delete cake");

    let cakes = Cake::find().all(db).await.unwrap();
    assert_eq!(cakes.len(), initial_cakes);
}

pub async fn test_delete_bakery(db: &DatabaseConnection) {
    let initial_bakeries = Bakery::find().all(db).await.unwrap().len();

    let bakery = bakery::ActiveModel {
        name: set("SeaSide Bakery"),
        profit_margin: set(10.4),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("could not insert bakery");

    assert_eq!(
        Bakery::find().all(db).await.unwrap().len(),
        initial_bakeries + 1
    );

    let _result = bakery.delete(db).await.expect("failed to delete bakery");

    assert_eq!(
        Bakery::find().all(db).await.unwrap().len(),
        initial_bakeries
    );
}
