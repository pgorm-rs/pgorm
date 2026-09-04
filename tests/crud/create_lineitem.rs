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
    let bakery_insert_res = Bakery::insert(seaside_bakery)
        .exec(db)
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
        bakery_id: set(Some(bakery_insert_res.last_insert_id)),
        ..Default::default()
    };
    let baker_insert_res = Baker::insert(baker_bob)
        .exec(db)
        .await
        .expect("could not insert baker");

    // Cake
    let mud_cake = cake::ActiveModel {
        name: set("Mud Cake"),
        price: set(rust_dec(10.25)),
        gluten_free: set(false),
        serial: set(Uuid::new_v4()),
        bakery_id: set(Some(bakery_insert_res.last_insert_id)),
        ..Default::default()
    };

    let cake_insert_res = Cake::insert(mud_cake)
        .exec(db)
        .await
        .expect("could not insert cake");

    // Cake_Baker
    let cake_baker = cakes_bakers::ActiveModel {
        cake_id: set(cake_insert_res.last_insert_id),
        baker_id: set(baker_insert_res.last_insert_id),
    };
    let cake_baker_res = CakesBakers::insert(cake_baker.clone())
        .exec(db)
        .await
        .expect("could not insert cake_baker");
    assert_eq!(
        cake_baker_res.last_insert_id,
        (cake_baker.cake_id.unwrap(), cake_baker.baker_id.unwrap())
    );

    // Customer
    let customer_kate = customer::ActiveModel {
        name: set("Kate"),
        notes: "Loves cheese cake".into(),
        ..Default::default()
    };
    let customer_insert_res = Customer::insert(customer_kate)
        .exec(db)
        .await
        .expect("could not insert customer");

    // Order
    let order_1 = order::ActiveModel {
        bakery_id: set(bakery_insert_res.last_insert_id),
        customer_id: set(customer_insert_res.last_insert_id),
        total: set(rust_dec(7.55)),
        placed_at: set(Utc::now().naive_utc()),
        ..Default::default()
    };
    let order_insert_res = Order::insert(order_1)
        .exec(db)
        .await
        .expect("could not insert order");

    // Lineitem
    let lineitem_1 = lineitem::ActiveModel {
        cake_id: set(cake_insert_res.last_insert_id),
        order_id: set(order_insert_res.last_insert_id),
        price: set(rust_dec(7.55)),
        quantity: set(1),
        ..Default::default()
    };
    let lineitem_insert_res = Lineitem::insert(lineitem_1)
        .exec(db)
        .await
        .expect("could not insert lineitem");

    let lineitem: Option<lineitem::Model> =
        Lineitem::find_by_id(lineitem_insert_res.last_insert_id)
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
    assert_eq!(order_model.customer_id, customer_insert_res.last_insert_id);
}
