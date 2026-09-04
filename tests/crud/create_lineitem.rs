pub use super::*;
use chrono::offset::Utc;
use pgorm::set;
use uuid::Uuid;

pub async fn test_create_lineitem(db: &DatabaseConnection) {
    // Bakery
    let seaside_bakery = bakery::ActiveModel {
        name: set("SeaSide Bakery"),
        profit_margin: set(10.4),
        ..Default::default()
    };
    let bakery_insert_res = Insert::one(seaside_bakery)
        .exec_returning_pk(db)
        .await
        .expect("could not insert bakery");

    // Baker
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

    // Cake
    let mud_cake = cake::ActiveModel {
        name: set("Mud Cake"),
        price: set(rust_dec(10.25)),
        gluten_free: set(false),
        serial: set(Uuid::new_v4()),
        bakery_id: set(Some(bakery_insert_res)),
        ..Default::default()
    };

    let cake_insert_res = Insert::one(mud_cake)
        .exec_returning_pk(db)
        .await
        .expect("could not insert cake");

    // Cake_Baker
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

    // Customer
    let customer_kate = customer::ActiveModel {
        name: set("Kate"),
        notes: "Loves cheese cake".into(),
        ..Default::default()
    };
    let customer_insert_res = Insert::one(customer_kate)
        .exec_returning_pk(db)
        .await
        .expect("could not insert customer");

    // Order
    let order_1 = order::ActiveModel {
        bakery_id: set(bakery_insert_res),
        customer_id: set(customer_insert_res),
        total: set(rust_dec(7.55)),
        placed_at: set(Utc::now().naive_utc()),
        ..Default::default()
    };
    let order_insert_res = Insert::one(order_1)
        .exec_returning_pk(db)
        .await
        .expect("could not insert order");

    // Lineitem
    let lineitem_1 = lineitem::ActiveModel {
        cake_id: set(cake_insert_res),
        order_id: set(order_insert_res),
        price: set(rust_dec(7.55)),
        quantity: set(1),
        ..Default::default()
    };
    let lineitem_insert_res = Insert::one(lineitem_1)
        .exec_returning_pk(db)
        .await
        .expect("could not insert lineitem");

    let lineitem: Option<lineitem::Model> = Lineitem::find_by_id(lineitem_insert_res)
        .one_opt(db)
        .await
        .expect("could not find lineitem");

    assert!(lineitem.is_some());
    let lineitem_model = lineitem.unwrap();

    assert_eq!(lineitem_model.price, rust_dec(7.55));

    let cake: Option<cake::Model> = Cake::find_by_id(lineitem_model.cake_id)
        .one_opt(db)
        .await
        .expect("could not find cake");

    let cake_model = cake.unwrap();
    assert_eq!(cake_model.name, "Mud Cake");

    let order: Option<order::Model> = Order::find_by_id(lineitem_model.order_id)
        .one_opt(db)
        .await
        .expect("could not find order");

    let order_model = order.unwrap();
    assert_eq!(order_model.customer_id, customer_insert_res);
}
