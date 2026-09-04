pub use super::*;
use pgorm::set;
use uuid::Uuid;

pub async fn test_create_cake(db: &DatabaseConnection) {
    let seaside_bakery = bakery::ActiveModel {
        name: set("SeaSide Bakery"),
        profit_margin: set(10.4),
        ..Default::default()
    };
    let bakery_insert_res = Insert::one(seaside_bakery)
        .exec_returning_pk(db)
        .await
        .expect("could not insert bakery");

    let baker_bob = baker::ActiveModel {
        name: set("Baker Bob"),
        contact_details: set(serde_json::json!({
            "mobile": "+61424000000",
            "home": "0395555555",
            "address": "12 Test St, Testville, Vic, Australia"
        })),
        bakery_id: set(Some(bakery_insert_res)),
        ..Default::default()
    };
    let baker_insert_res = Insert::one(baker_bob)
        .exec_returning_pk(db)
        .await
        .expect("could not insert baker");
    let uuid = Uuid::new_v4();

    let mud_cake = cake::ActiveModel {
        name: set("Mud Cake"),
        price: set(rust_dec(-10.25)),
        gluten_free: set(false),
        serial: set(uuid),
        bakery_id: set(Some(bakery_insert_res)),
        ..Default::default()
    };

    let cake_insert_res = Insert::one(mud_cake)
        .exec_returning_pk(db)
        .await
        .expect("could not insert cake");

    let cake: Option<cake::Model> = Cake::find_by_id(cake_insert_res)
        .one_opt(db)
        .await
        .expect("could not find cake");

    let cake_baker = cakes_bakers::ActiveModel {
        cake_id: set(cake_insert_res),
        baker_id: set(baker_insert_res),
    };
    let cake_baker_res = Insert::one(cake_baker.clone())
        .exec_returning_pk(db)
        .await
        .expect("could not insert cake_baker");
    assert_eq!(
        cake_baker_res,
        (cake_baker.cake_id.unwrap(), cake_baker.baker_id.unwrap())
    );

    assert!(cake.is_some());
    let cake_model = cake.unwrap();
    assert_eq!(cake_model.name, "Mud Cake");
    assert_eq!(cake_model.price, rust_dec(-10.25));
    assert!(!cake_model.gluten_free);
    assert_eq!(
        cake_model
            .find_related(Bakery)
            .one(db)
            .await
            .expect("Bakery not found")
            .name,
        "SeaSide Bakery"
    );
    assert_eq!(cake_model.serial, uuid);

    let related_bakers: Vec<baker::Model> = cake_model
        .find_related(Baker)
        .all(db)
        .await
        .expect("could not find related bakers");
    assert_eq!(related_bakers.len(), 1);
    assert_eq!(related_bakers[0].name, "Baker Bob");

    let baker: Option<baker::Model> = Baker::find_by_id(baker_insert_res)
        .one_opt(db)
        .await
        .expect("could not find baker");

    let related_cakes: Vec<cake::Model> = baker
        .unwrap()
        .find_related(Cake)
        .all(db)
        .await
        .expect("could not find related cakes");
    assert_eq!(related_cakes.len(), 1);
    assert_eq!(related_cakes[0].name, "Mud Cake")
}
