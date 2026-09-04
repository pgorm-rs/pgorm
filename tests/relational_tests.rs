#![allow(unused_imports, dead_code)]

pub mod common;

pub use chrono::offset::Utc;
pub use common::{TestContext, bakery_chain::*, setup::*};
use pgorm::{DerivePartialModel, Error, FromQueryResult, entity::*, query::*, set};
use pgorm_query::{Expr, Func, SimpleExpr};
use pretty_assertions::assert_eq;
pub use rust_decimal::prelude::*;
pub use uuid::Uuid;

// Run the test locally:
// DATABASE_URL="postgres://postgres:postgres@localhost" cargo test --test relational_tests
#[pgorm_macros::test]
pub async fn left_join() {
    let ctx = TestContext::new("test_left_join").await;
    create_tables(&ctx.db).await.unwrap();

    let db = ctx.db.get().await.unwrap();

    let bakery = bakery::ActiveModel {
        name: set("SeaSide Bakery"),
        profit_margin: set(10.4),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert bakery");

    let _baker_1 = baker::ActiveModel {
        name: set("Baker 1"),
        contact_details: set(serde_json::json!({
            "mobile": "+61424000000",
            "home": "0395555555",
            "address": "12 Test St, Testville, Vic, Australia"
        })),
        bakery_id: set(Some(bakery.id)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert baker");

    let _baker_2 = baker::ActiveModel {
        name: set("Baker 2"),
        contact_details: set(serde_json::json!({})),
        bakery_id: set(None),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert baker");

    #[derive(Debug, FromQueryResult)]
    struct SelectResult {
        name: String,
        bakery_name: Option<String>,
    }

    let select = baker::Entity::find()
        .left_join(bakery::Entity)
        .select_only()
        .column(baker::Column::Name)
        .column_as(bakery::Column::Name, "bakery_name")
        .filter(baker::Column::Name.contains("Baker 1"));

    let result = select
        .clone()
        .into_model::<SelectResult>()
        .one(&db)
        .await
        .unwrap();
    assert_eq!(result.name.as_str(), "Baker 1");
    assert_eq!(result.bakery_name, Some("SeaSide Bakery".to_string()));

    #[derive(DerivePartialModel, FromQueryResult, Debug, PartialEq)]
    #[pgorm(entity = "Baker")]
    struct PartialSelectResult {
        name: String,
        #[pgorm(from_expr = "Expr::col((bakery::Entity, bakery::Column::Name))")]
        bakery_name: Option<String>,
        #[pgorm(
            from_expr = r#"SimpleExpr::FunctionCall(Func::upper(Expr::col((bakery::Entity, bakery::Column::Name))))"#
        )]
        bakery_name_upper: Option<String>,
    }

    let result = select
        .into_partial_model::<PartialSelectResult>()
        .one(&db)
        .await
        .unwrap();
    assert_eq!(result.name.as_str(), "Baker 1");
    assert_eq!(result.bakery_name, Some("SeaSide Bakery".to_string()));
    assert_eq!(result.bakery_name_upper, Some("SEASIDE BAKERY".to_string()));

    let select = baker::Entity::find()
        .left_join(bakery::Entity)
        .select_only()
        .column(baker::Column::Name)
        .column_as(bakery::Column::Name, "bakery_name")
        .filter(baker::Column::Name.contains("Baker 2"));

    let result = select.into_model::<SelectResult>().one(&db).await.unwrap();
    assert_eq!(result.bakery_name, None);

    drop(db);
    ctx.delete().await;
}

#[pgorm_macros::test]
pub async fn right_join() {
    let ctx = TestContext::new("test_right_join").await;
    create_tables(&ctx.db).await.unwrap();

    let db = ctx.db.get().await.unwrap();

    let bakery = bakery::ActiveModel {
        name: set("SeaSide Bakery"),
        profit_margin: set(10.4),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert bakery");

    let customer_kate = customer::ActiveModel {
        name: set("Kate"),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert customer");

    let _customer_jim = customer::ActiveModel {
        name: set("Jim"),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert customer");

    let _order = order::ActiveModel {
        bakery_id: set(bakery.id),
        customer_id: set(customer_kate.id),
        total: set(rust_dec(15.10)),
        placed_at: set(Utc::now().naive_utc()),

        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert order");

    #[derive(FromQueryResult)]
    #[allow(dead_code)]
    struct SelectResult {
        name: String,
        order_total: Option<Decimal>,
    }

    let select = order::Entity::find()
        .right_join(customer::Entity)
        .select_only()
        .column(customer::Column::Name)
        .column_as(order::Column::Total, "order_total")
        .filter(customer::Column::Name.contains("Kate"));

    let result = select.into_model::<SelectResult>().one(&db).await.unwrap();
    assert_eq!(result.order_total, Some(rust_dec(15.10)));

    let select = order::Entity::find()
        .right_join(customer::Entity)
        .select_only()
        .column(customer::Column::Name)
        .column_as(order::Column::Total, "order_total")
        .filter(customer::Column::Name.contains("Jim"));

    let result = select.into_model::<SelectResult>().one(&db).await.unwrap();
    assert_eq!(result.order_total, None);

    drop(db);
    ctx.delete().await;
}

#[pgorm_macros::test]
pub async fn inner_join() {
    let ctx = TestContext::new("test_inner_join").await;
    create_tables(&ctx.db).await.unwrap();

    let db = ctx.db.get().await.unwrap();

    let bakery = bakery::ActiveModel {
        name: set("SeaSide Bakery"),
        profit_margin: set(10.4),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert bakery");

    let customer_kate = customer::ActiveModel {
        name: set("Kate"),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert customer");

    let _customer_jim = customer::ActiveModel {
        name: set("Jim"),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert customer");

    let kate_order_1 = order::ActiveModel {
        bakery_id: set(bakery.id),
        customer_id: set(customer_kate.id),
        total: set(rust_dec(15.10)),
        placed_at: set(Utc::now().naive_utc()),

        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert order");

    let kate_order_2 = order::ActiveModel {
        bakery_id: set(bakery.id),
        customer_id: set(customer_kate.id),
        total: set(rust_dec(100.00)),
        placed_at: set(Utc::now().naive_utc()),

        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert order");

    #[derive(Debug, FromQueryResult)]
    struct SelectResult {
        name: String,
        order_total: Option<Decimal>,
    }

    let select = order::Entity::find()
        .inner_join(customer::Entity)
        .select_only()
        .column(customer::Column::Name)
        .column_as(order::Column::Total, "order_total");

    let results = select.into_model::<SelectResult>().all(&db).await.unwrap();

    assert_eq!(results.len(), 2);
    assert!(
        results
            .iter()
            .any(|result| result.name == customer_kate.name.clone()
                && result.order_total == Some(kate_order_1.total))
    );
    assert!(
        results
            .iter()
            .any(|result| result.name == customer_kate.name.clone()
                && result.order_total == Some(kate_order_2.total))
    );

    drop(db);
    ctx.delete().await;
}

#[pgorm_macros::test]
pub async fn group_by() {
    let ctx = TestContext::new("test_group_by").await;
    create_tables(&ctx.db).await.unwrap();

    let db = ctx.db.get().await.unwrap();

    let bakery = bakery::ActiveModel {
        name: set("SeaSide Bakery"),
        profit_margin: set(10.4),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert bakery");

    let customer_kate = customer::ActiveModel {
        name: set("Kate"),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert customer");

    let kate_order_1 = order::ActiveModel {
        bakery_id: set(bakery.id),
        customer_id: set(customer_kate.id),
        total: set(rust_dec(99.95)),
        placed_at: set(Utc::now().naive_utc()),

        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert order");

    let kate_order_2 = order::ActiveModel {
        bakery_id: set(bakery.id),
        customer_id: set(customer_kate.id),
        total: set(rust_dec(200.00)),
        placed_at: set(Utc::now().naive_utc()),

        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert order");

    #[derive(Debug, FromQueryResult)]
    struct SelectResult {
        name: String,
        number_orders: Option<i64>,
        total_spent: Option<Decimal>,
        min_spent: Option<Decimal>,
        max_spent: Option<Decimal>,
    }

    let select = customer::Entity::find()
        .left_join(order::Entity)
        .select_only()
        .column(customer::Column::Name)
        .column_as(order::Column::Total.count(), "number_orders")
        .column_as(order::Column::Total.sum(), "total_spent")
        .column_as(order::Column::Total.min(), "min_spent")
        .column_as(order::Column::Total.max(), "max_spent")
        .group_by(customer::Column::Name);

    let result = select.into_model::<SelectResult>().one(&db).await.unwrap();

    assert_eq!(result.name.as_str(), "Kate");
    assert_eq!(result.number_orders, Some(2));
    assert_eq!(
        result.total_spent,
        Some(kate_order_1.total + kate_order_2.total)
    );
    assert_eq!(
        result.min_spent,
        Some(kate_order_1.total.min(kate_order_2.total))
    );
    assert_eq!(
        result.max_spent,
        Some(kate_order_1.total.max(kate_order_2.total))
    );
    drop(db);
    ctx.delete().await;
}

#[pgorm_macros::test]
pub async fn having() {
    // customers with orders with total equal to $90
    let ctx = TestContext::new("test_having").await;
    create_tables(&ctx.db).await.unwrap();

    let db = ctx.db.get().await.unwrap();

    let bakery = bakery::ActiveModel {
        name: set("SeaSide Bakery"),
        profit_margin: set(10.4),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert bakery");

    let customer_kate = customer::ActiveModel {
        name: set("Kate"),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert customer");

    let kate_order_1 = order::ActiveModel {
        bakery_id: set(bakery.id),
        customer_id: set(customer_kate.id),
        total: set(rust_dec(100.00)),
        placed_at: set(Utc::now().naive_utc()),

        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert order");

    let _kate_order_2 = order::ActiveModel {
        bakery_id: set(bakery.id),
        customer_id: set(customer_kate.id),
        total: set(rust_dec(12.00)),
        placed_at: set(Utc::now().naive_utc()),

        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert order");

    let customer_bob = customer::ActiveModel {
        name: set("Bob"),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert customer");

    let _bob_order_1 = order::ActiveModel {
        bakery_id: set(bakery.id),
        customer_id: set(customer_bob.id),
        total: set(rust_dec(50.0)),
        placed_at: set(Utc::now().naive_utc()),

        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert order");

    let _bob_order_2 = order::ActiveModel {
        bakery_id: set(bakery.id),
        customer_id: set(customer_bob.id),
        total: set(rust_dec(50.0)),
        placed_at: set(Utc::now().naive_utc()),

        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("could not insert order");

    #[derive(Debug, FromQueryResult)]
    struct SelectResult {
        name: String,
        order_total: Option<Decimal>,
    }

    let results = customer::Entity::find()
        .inner_join(order::Entity)
        .select_only()
        .column(customer::Column::Name)
        .column_as(order::Column::Total, "order_total")
        .group_by(customer::Column::Name)
        .group_by(order::Column::Total)
        .having(order::Column::Total.gt(rust_dec(90.00)))
        .into_model::<SelectResult>()
        .all(&db)
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, customer_kate.name.clone());
    assert_eq!(results[0].order_total, Some(kate_order_1.total));

    drop(db);
    ctx.delete().await;
}

// [spec:pgorm:def:exec.crud/test]    `SelectTwoModel` decoding `(M, Option<N>)`
// through the `SelectA` / `SelectB` column prefixes
// [spec:pgorm:sem:exec.crud.consolidate/test]    `SelectTwoMany::all` grouping
// on a unary primary key: children in row order, one entry per left key, and an
// empty `Vec` for a left row with no right model
// [spec:pgorm:req:entity.relation+1/test]    `Related::find_related` inner-joins
// `to()` onto a fresh `Select<R>`, exercised against real rows
// [spec:pgorm:def:entity.traits.model+3/test]    `ModelTrait::find_related` scopes
// that select to a single model instance
#[pgorm_macros::test]
pub async fn related() -> Result<(), Error> {
    use pgorm::{SelectA, SelectB};

    let ctx = TestContext::new("test_related").await;
    create_tables(&ctx.db).await?;

    let db = ctx.db.get().await?;

    // SeaSide Bakery
    let seaside_bakery = bakery::ActiveModel {
        name: set("SeaSide Bakery"),
        profit_margin: set(10.4),
        ..Default::default()
    };
    let seaside_bakery_res = Bakery::insert(seaside_bakery).exec(&db).await?;

    // Bob's Baker
    let baker_bob = baker::ActiveModel {
        name: set("Baker Bob"),
        contact_details: set(serde_json::json!({
            "mobile": "+61424000000",
            "home": "0395555555",
            "address": "12 Test St, Testville, Vic, Australia"
        })),
        bakery_id: set(Some(seaside_bakery_res.last_insert_id)),
        ..Default::default()
    };
    let _baker_bob_res = Baker::insert(baker_bob).exec(&db).await?;

    // Bobby's Baker
    let baker_bobby = baker::ActiveModel {
        name: set("Baker Bobby"),
        contact_details: set(serde_json::json!({
            "mobile": "+85212345678",
        })),
        bakery_id: set(Some(seaside_bakery_res.last_insert_id)),
        ..Default::default()
    };
    let _baker_bobby_res = Baker::insert(baker_bobby).exec(&db).await?;

    // Terres Bakery
    let terres_bakery = bakery::ActiveModel {
        name: set("Terres Bakery"),
        profit_margin: set(13.5),
        ..Default::default()
    };
    let terres_bakery_res = Bakery::insert(terres_bakery).exec(&db).await?;

    // Ada's Baker
    let baker_ada = baker::ActiveModel {
        name: set("Baker Ada"),
        contact_details: set(serde_json::json!({
            "mobile": "+61424000000",
            "home": "0395555555",
            "address": "12 Test St, Testville, Vic, Australia"
        })),
        bakery_id: set(Some(terres_bakery_res.last_insert_id)),
        ..Default::default()
    };
    let _baker_ada_res = Baker::insert(baker_ada).exec(&db).await?;

    // Stone Bakery, with no baker
    let stone_bakery = bakery::ActiveModel {
        name: set("Stone Bakery"),
        profit_margin: set(13.5),
        ..Default::default()
    };
    let _stone_bakery_res = Bakery::insert(stone_bakery).exec(&db).await?;

    #[derive(Debug, FromQueryResult, PartialEq)]
    struct BakerLite {
        name: String,
    }

    #[derive(Debug, FromQueryResult, PartialEq)]
    struct BakeryLite {
        name: String,
    }

    // get all bakery and baker's name and put them into tuples
    let bakers_in_bakery: Vec<(BakeryLite, Option<BakerLite>)> = Bakery::find()
        .find_also_related(Baker)
        .select_only()
        .column_as(bakery::Column::Name, (SelectA, bakery::Column::Name))
        .column_as(baker::Column::Name, (SelectB, baker::Column::Name))
        .order_by_asc(bakery::Column::Id)
        .order_by_asc(baker::Column::Id)
        .into_model()
        .all(&db)
        .await?;

    assert_eq!(
        bakers_in_bakery,
        [
            (
                BakeryLite {
                    name: "SeaSide Bakery".to_owned(),
                },
                Some(BakerLite {
                    name: "Baker Bob".to_owned(),
                })
            ),
            (
                BakeryLite {
                    name: "SeaSide Bakery".to_owned(),
                },
                Some(BakerLite {
                    name: "Baker Bobby".to_owned(),
                })
            ),
            (
                BakeryLite {
                    name: "Terres Bakery".to_owned(),
                },
                Some(BakerLite {
                    name: "Baker Ada".to_owned(),
                })
            ),
            (
                BakeryLite {
                    name: "Stone Bakery".to_owned(),
                },
                None,
            ),
        ]
    );

    let seaside_bakery = Bakery::find()
        .filter(bakery::Column::Id.eq(1))
        .one(&db)
        .await?;

    let bakers = seaside_bakery.find_related(Baker).all(&db).await?;

    assert_eq!(
        bakers,
        [
            baker::Model {
                id: 1,
                name: "Baker Bob".to_owned(),
                contact_details: serde_json::json!({
                    "mobile": "+61424000000",
                    "home": "0395555555",
                    "address": "12 Test St, Testville, Vic, Australia"
                }),
                bakery_id: Some(1),
            },
            baker::Model {
                id: 2,
                name: "Baker Bobby".to_owned(),
                contact_details: serde_json::json!({
                    "mobile": "+85212345678",
                }),
                bakery_id: Some(1),
            }
        ]
    );

    let select_bakery_with_baker = Bakery::find()
        .find_with_related(Baker)
        .order_by_asc(baker::Column::Id);

    assert_eq!(
        select_bakery_with_baker.build().0,
        [
            r#"SELECT "bakery"."id" AS "A_id","#,
            r#""bakery"."name" AS "A_name","#,
            r#""bakery"."profit_margin" AS "A_profit_margin","#,
            r#""baker"."id" AS "B_id","#,
            r#""baker"."name" AS "B_name","#,
            r#""baker"."contact_details" AS "B_contact_details","#,
            r#""baker"."bakery_id" AS "B_bakery_id""#,
            r#"FROM "bakery""#,
            r#"LEFT JOIN "baker" ON "bakery"."id" = "baker"."bakery_id""#,
            r#"ORDER BY "bakery"."id" ASC, "baker"."id" ASC"#
        ]
        .join(" ")
    );

    assert_eq!(
        select_bakery_with_baker.all(&db).await?,
        [
            (
                bakery::Model {
                    id: 1,
                    name: "SeaSide Bakery".to_owned(),
                    profit_margin: 10.4,
                },
                vec![
                    baker::Model {
                        id: 1,
                        name: "Baker Bob".to_owned(),
                        contact_details: serde_json::json!({
                            "mobile": "+61424000000",
                            "home": "0395555555",
                            "address": "12 Test St, Testville, Vic, Australia"
                        }),
                        bakery_id: Some(seaside_bakery_res.last_insert_id),
                    },
                    baker::Model {
                        id: 2,
                        name: "Baker Bobby".to_owned(),
                        contact_details: serde_json::json!({
                            "mobile": "+85212345678",
                        }),
                        bakery_id: Some(seaside_bakery_res.last_insert_id),
                    }
                ]
            ),
            (
                bakery::Model {
                    id: 2,
                    name: "Terres Bakery".to_owned(),
                    profit_margin: 13.5,
                },
                vec![baker::Model {
                    id: 3,
                    name: "Baker Ada".to_owned(),
                    contact_details: serde_json::json!({
                        "mobile": "+61424000000",
                        "home": "0395555555",
                        "address": "12 Test St, Testville, Vic, Australia"
                    }),
                    bakery_id: Some(terres_bakery_res.last_insert_id),
                }]
            ),
            (
                bakery::Model {
                    id: 3,
                    name: "Stone Bakery".to_owned(),
                    profit_margin: 13.5,
                },
                vec![]
            ),
        ]
    );

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:req:entity.relation.linked/test]    a five-hop `Linked` chain
// resolving to the `r0`..`r4` alias ladder, and `ModelTrait::find_linked`
// filtering on the final alias, both verified against real rows
// [spec:pgorm:def:entity.traits.model+3/test]    `ModelTrait::find_linked` scopes
// a multi-hop join to one model instance
#[pgorm_macros::test]
pub async fn linked() -> Result<(), Error> {
    use common::bakery_chain::Order;
    use pgorm::{SelectA, SelectB};
    use pgorm_query::{Alias, Expr};

    let ctx = TestContext::new("test_linked").await;
    create_tables(&ctx.db).await?;

    let db = ctx.db.get().await?;

    // SeaSide Bakery
    let seaside_bakery = bakery::ActiveModel {
        name: set("SeaSide Bakery"),
        profit_margin: set(10.4),
        ..Default::default()
    };
    let seaside_bakery_res = Bakery::insert(seaside_bakery).exec(&db).await?;

    // Bob's Baker, Cake & Cake Baker
    let baker_bob = baker::ActiveModel {
        name: set("Baker Bob"),
        contact_details: set(serde_json::json!({
            "mobile": "+61424000000",
            "home": "0395555555",
            "address": "12 Test St, Testville, Vic, Australia"
        })),
        bakery_id: set(Some(seaside_bakery_res.last_insert_id)),
        ..Default::default()
    };
    let baker_bob_res = Baker::insert(baker_bob).exec(&db).await?;
    let mud_cake = cake::ActiveModel {
        name: set("Mud Cake"),
        price: set(rust_dec(10.25)),
        gluten_free: set(false),
        serial: set(Uuid::new_v4()),
        bakery_id: set(Some(seaside_bakery_res.last_insert_id)),
        ..Default::default()
    };
    let mud_cake_res = Cake::insert(mud_cake).exec(&db).await?;
    let bob_cakes_bakers = cakes_bakers::ActiveModel {
        cake_id: set(mud_cake_res.last_insert_id),
        baker_id: set(baker_bob_res.last_insert_id),
    };
    CakesBakers::insert(bob_cakes_bakers).exec(&db).await?;

    // Bobby's Baker, Cake & Cake Baker
    let baker_bobby = baker::ActiveModel {
        name: set("Baker Bobby"),
        contact_details: set(serde_json::json!({
            "mobile": "+85212345678",
        })),
        bakery_id: set(Some(seaside_bakery_res.last_insert_id)),
        ..Default::default()
    };
    let baker_bobby_res = Baker::insert(baker_bobby).exec(&db).await?;
    let cheese_cake = cake::ActiveModel {
        name: set("Cheese Cake"),
        price: set(rust_dec(20.5)),
        gluten_free: set(false),
        serial: set(Uuid::new_v4()),
        bakery_id: set(Some(seaside_bakery_res.last_insert_id)),
        ..Default::default()
    };
    let cheese_cake_res = Cake::insert(cheese_cake).exec(&db).await?;
    let bobby_cakes_bakers = cakes_bakers::ActiveModel {
        cake_id: set(cheese_cake_res.last_insert_id),
        baker_id: set(baker_bobby_res.last_insert_id),
    };
    CakesBakers::insert(bobby_cakes_bakers).exec(&db).await?;
    let chocolate_cake = cake::ActiveModel {
        name: set("Chocolate Cake"),
        price: set(rust_dec(30.15)),
        gluten_free: set(false),
        serial: set(Uuid::new_v4()),
        bakery_id: set(Some(seaside_bakery_res.last_insert_id)),
        ..Default::default()
    };
    let chocolate_cake_res = Cake::insert(chocolate_cake).exec(&db).await?;
    let bobby_cakes_bakers = cakes_bakers::ActiveModel {
        cake_id: set(chocolate_cake_res.last_insert_id),
        baker_id: set(baker_bobby_res.last_insert_id),
    };
    CakesBakers::insert(bobby_cakes_bakers).exec(&db).await?;

    // Freerider's Baker, no cake baked
    let baker_freerider = baker::ActiveModel {
        name: set("Freerider"),
        contact_details: set(serde_json::json!({
            "mobile": "+85298765432",
        })),
        bakery_id: set(Some(seaside_bakery_res.last_insert_id)),
        ..Default::default()
    };
    let _baker_freerider_res = Baker::insert(baker_freerider).exec(&db).await?;

    // Kate's Customer, Order & Line Item
    let customer_kate = customer::ActiveModel {
        name: set("Kate"),
        notes: "Loves cheese cake".into(),
        ..Default::default()
    };
    let customer_kate_res = Customer::insert(customer_kate).exec(&db).await?;
    let kate_order_1 = order::ActiveModel {
        bakery_id: set(seaside_bakery_res.last_insert_id),
        customer_id: set(customer_kate_res.last_insert_id),
        total: set(rust_dec(15.10)),
        placed_at: set(Utc::now().naive_utc()),
        ..Default::default()
    };
    let kate_order_1_res = Order::insert(kate_order_1).exec(&db).await?;
    lineitem::ActiveModel {
        cake_id: set(cheese_cake_res.last_insert_id),
        order_id: set(kate_order_1_res.last_insert_id),
        price: set(rust_dec(7.55)),
        quantity: set(2),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let kate_order_2 = order::ActiveModel {
        bakery_id: set(seaside_bakery_res.last_insert_id),
        customer_id: set(customer_kate_res.last_insert_id),
        total: set(rust_dec(29.7)),
        placed_at: set(Utc::now().naive_utc()),
        ..Default::default()
    };
    let kate_order_2_res = Order::insert(kate_order_2).exec(&db).await?;
    lineitem::ActiveModel {
        cake_id: set(chocolate_cake_res.last_insert_id),
        order_id: set(kate_order_2_res.last_insert_id),
        price: set(rust_dec(9.9)),
        quantity: set(3),
        ..Default::default()
    }
    .insert(&db)
    .await?;

    // Kara's Customer, Order & Line Item
    let customer_kara = customer::ActiveModel {
        name: set("Kara"),
        notes: "Loves all cakes".into(),
        ..Default::default()
    };
    let customer_kara_res = Customer::insert(customer_kara).exec(&db).await?;
    let kara_order_1 = order::ActiveModel {
        bakery_id: set(seaside_bakery_res.last_insert_id),
        customer_id: set(customer_kara_res.last_insert_id),
        total: set(rust_dec(15.10)),
        placed_at: set(Utc::now().naive_utc()),
        ..Default::default()
    };
    let kara_order_1_res = Order::insert(kara_order_1).exec(&db).await?;
    lineitem::ActiveModel {
        cake_id: set(mud_cake_res.last_insert_id),
        order_id: set(kara_order_1_res.last_insert_id),
        price: set(rust_dec(7.55)),
        quantity: set(2),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    let kara_order_2 = order::ActiveModel {
        bakery_id: set(seaside_bakery_res.last_insert_id),
        customer_id: set(customer_kara_res.last_insert_id),
        total: set(rust_dec(29.7)),
        placed_at: set(Utc::now().naive_utc()),
        ..Default::default()
    };
    let kara_order_2_res = Order::insert(kara_order_2).exec(&db).await?;
    lineitem::ActiveModel {
        cake_id: set(cheese_cake_res.last_insert_id),
        order_id: set(kara_order_2_res.last_insert_id),
        price: set(rust_dec(9.9)),
        quantity: set(3),
        ..Default::default()
    }
    .insert(&db)
    .await?;

    #[derive(Debug, FromQueryResult, PartialEq)]
    struct BakerLite {
        name: String,
    }

    #[derive(Debug, FromQueryResult, PartialEq)]
    struct CustomerLite {
        name: String,
    }

    // filtered find
    let baked_for_customers: Vec<(BakerLite, Option<CustomerLite>)> = Baker::find()
        .find_also_linked(baker::BakedForCustomer)
        .select_only()
        .column_as(baker::Column::Name, (SelectA, baker::Column::Name))
        .column_as(
            Expr::col((Alias::new("r4"), customer::Column::Name)),
            (SelectB, customer::Column::Name),
        )
        .group_by(baker::Column::Id)
        .group_by(Expr::col((Alias::new("r4"), customer::Column::Id)))
        .group_by(baker::Column::Name)
        .group_by(Expr::col((Alias::new("r4"), customer::Column::Name)))
        .order_by_asc(baker::Column::Id)
        .order_by_asc(Expr::col((Alias::new("r4"), customer::Column::Id)))
        .into_model()
        .all(&db)
        .await?;

    assert_eq!(
        baked_for_customers,
        [
            (
                BakerLite {
                    name: "Baker Bob".to_owned(),
                },
                Some(CustomerLite {
                    name: "Kara".to_owned(),
                })
            ),
            (
                BakerLite {
                    name: "Baker Bobby".to_owned(),
                },
                Some(CustomerLite {
                    name: "Kate".to_owned(),
                })
            ),
            (
                BakerLite {
                    name: "Baker Bobby".to_owned(),
                },
                Some(CustomerLite {
                    name: "Kara".to_owned(),
                })
            ),
            (
                BakerLite {
                    name: "Freerider".to_owned(),
                },
                None,
            ),
        ]
    );

    // try to use find_linked instead
    let baker_bob = Baker::find()
        .filter(baker::Column::Id.eq(1))
        .one(&db)
        .await?;

    let baker_bob_customers = baker_bob
        .find_linked(baker::BakedForCustomer)
        .all(&db)
        .await?;

    assert_eq!(
        baker_bob_customers,
        [customer::Model {
            id: 2,
            name: "Kara".to_owned(),
            notes: Some("Loves all cakes".to_owned()),
        }]
    );

    // find full model using with_linked
    let select_baker_with_customer = Baker::find()
        .find_with_linked(baker::BakedForCustomer)
        .order_by_asc(baker::Column::Id)
        .order_by_asc(Expr::col((Alias::new("r4"), customer::Column::Id)));

    assert_eq!(
        select_baker_with_customer.build().0,
        [
            r#"SELECT "baker"."id" AS "A_id","#,
            r#""baker"."name" AS "A_name","#,
            r#""baker"."contact_details" AS "A_contact_details","#,
            r#""baker"."bakery_id" AS "A_bakery_id","#,
            r#""r4"."id" AS "B_id","#,
            r#""r4"."name" AS "B_name","#,
            r#""r4"."notes" AS "B_notes""#,
            r#"FROM "baker""#,
            r#"LEFT JOIN "cakes_bakers" AS "r0" ON "baker"."id" = "r0"."baker_id""#,
            r#"LEFT JOIN "cake" AS "r1" ON "r0"."cake_id" = "r1"."id""#,
            r#"LEFT JOIN "lineitem" AS "r2" ON "r1"."id" = "r2"."cake_id""#,
            r#"LEFT JOIN "order" AS "r3" ON "r2"."order_id" = "r3"."id""#,
            r#"LEFT JOIN "customer" AS "r4" ON "r3"."customer_id" = "r4"."id""#,
            r#"ORDER BY "baker"."id" ASC, "r4"."id" ASC"#
        ]
        .join(" ")
    );

    assert_eq!(
        select_baker_with_customer.all(&db).await?,
        [
            (
                baker::Model {
                    id: 1,
                    name: "Baker Bob".into(),
                    contact_details: serde_json::json!({
                        "mobile": "+61424000000",
                        "home": "0395555555",
                        "address": "12 Test St, Testville, Vic, Australia",
                    }),
                    bakery_id: Some(1),
                },
                vec![customer::Model {
                    id: 2,
                    name: "Kara".into(),
                    notes: Some("Loves all cakes".into()),
                }]
            ),
            (
                baker::Model {
                    id: 2,
                    name: "Baker Bobby".into(),
                    contact_details: serde_json::json!({
                        "mobile": "+85212345678",
                    }),
                    bakery_id: Some(1),
                },
                vec![
                    customer::Model {
                        id: 1,
                        name: "Kate".into(),
                        notes: Some("Loves cheese cake".into()),
                    },
                    customer::Model {
                        id: 1,
                        name: "Kate".into(),
                        notes: Some("Loves cheese cake".into()),
                    },
                    customer::Model {
                        id: 2,
                        name: "Kara".into(),
                        notes: Some("Loves all cakes".into()),
                    },
                ]
            ),
            (
                baker::Model {
                    id: 3,
                    name: "Freerider".into(),
                    contact_details: serde_json::json!({
                        "mobile": "+85298765432",
                    }),
                    bakery_id: Some(1),
                },
                vec![]
            ),
        ]
    );

    drop(db);
    ctx.delete().await;

    Ok(())
}

mod composite_parent {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "composite_parent")]
    pub struct Model {
        #[pgorm(primary_key, auto_increment = false)]
        pub region: i32,
        #[pgorm(primary_key, auto_increment = false)]
        pub code: i32,
        pub name: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {
        Child,
    }

    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            match self {
                Self::Child => Entity::has_many(super::composite_child::Entity)
                    .columns(Column::Region, super::composite_child::Column::ParentRegion)
                    .and_columns(Column::Code, super::composite_child::Column::ParentCode)
                    .into(),
            }
        }
    }

    impl Related<super::composite_child::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Child.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

mod composite_child {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "composite_child")]
    pub struct Model {
        #[pgorm(primary_key, auto_increment = false)]
        pub id: i32,
        pub parent_region: i32,
        pub parent_code: i32,
        pub label: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {
        Parent,
    }

    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            match self {
                Self::Parent => Entity::belongs_to(super::composite_parent::Entity)
                    .columns(
                        Column::ParentRegion,
                        super::composite_parent::Column::Region,
                    )
                    .and_columns(Column::ParentCode, super::composite_parent::Column::Code)
                    .into(),
            }
        }
    }

    impl Related<super::composite_parent::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Parent.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

// [spec:pgorm:sem:exec.crud.consolidate/test]    grouping by a composite
// (pair-arity) left primary key, in row order, with a childless parent still
// producing an entry
#[pgorm_macros::test]
pub async fn consolidate_composite_key() -> Result<(), Error> {
    use pgorm::{QueryOrder, Schema};

    let ctx = TestContext::new("test_consolidate_composite_key").await;
    let db = ctx.db.get().await?;

    let schema = Schema::new();
    create_table_without_asserts(
        &db,
        &schema.create_table_from_entity(composite_parent::Entity),
    )
    .await?;
    create_table_without_asserts(
        &db,
        &schema.create_table_from_entity(composite_child::Entity),
    )
    .await?;

    // Two parents share a `region`, two share a `code`: only the pair
    // distinguishes them.
    let parents = [(1, 1, "one-one"), (1, 2, "one-two"), (2, 1, "two-one")];
    for (region, code, name) in parents {
        composite_parent::ActiveModel {
            region: set(region),
            code: set(code),
            name: set(name),
        }
        .insert(&db)
        .await?;
    }

    let children = [
        (1, 1, 1, "a"),
        (2, 1, 2, "b"),
        (3, 2, 1, "c"),
        (4, 1, 1, "d"),
    ];
    for (id, parent_region, parent_code, label) in children {
        composite_child::ActiveModel {
            id: set(id),
            parent_region: set(parent_region),
            parent_code: set(parent_code),
            label: set(label),
        }
        .insert(&db)
        .await?;
    }

    let child =
        |id: i32, parent_region: i32, parent_code: i32, label: &str| composite_child::Model {
            id,
            parent_region,
            parent_code,
            label: label.to_owned(),
        };
    let parent = |region: i32, code: i32, name: &str| composite_parent::Model {
        region,
        code,
        name: name.to_owned(),
    };

    let consolidated = composite_parent::Entity::find()
        .find_with_related(composite_child::Entity)
        .order_by_asc(composite_parent::Column::Region)
        .order_by_asc(composite_parent::Column::Code)
        .order_by_asc(composite_child::Column::Id)
        .all(&db)
        .await?;

    assert_eq!(
        consolidated,
        [
            (
                parent(1, 1, "one-one"),
                vec![child(1, 1, 1, "a"), child(4, 1, 1, "d")],
            ),
            (parent(1, 2, "one-two"), vec![child(2, 1, 2, "b")]),
            (parent(2, 1, "two-one"), vec![child(3, 2, 1, "c")]),
        ]
    );

    // A parent with no children still yields exactly one entry, with an empty
    // child vector.
    composite_parent::ActiveModel {
        region: set(3),
        code: set(3),
        name: set("lonely"),
    }
    .insert(&db)
    .await?;

    let consolidated = composite_parent::Entity::find()
        .find_with_related(composite_child::Entity)
        .order_by_asc(composite_parent::Column::Region)
        .order_by_asc(composite_parent::Column::Code)
        .order_by_asc(composite_child::Column::Id)
        .all(&db)
        .await?;

    assert_eq!(consolidated.len(), 4);
    assert_eq!(consolidated[3], (parent(3, 3, "lonely"), vec![]));

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:query.build.join+2/test]    a composite relation constrains
// the join on every declared pair: against rows where each column alone is
// ambiguous, the join still matches exactly one parent per child
#[pgorm_macros::test]
pub async fn composite_join_constrains_both_columns() -> Result<(), Error> {
    use pgorm::{QueryOrder, Schema};
    use pgorm_query::QueryBuilder;

    let ctx = TestContext::new("test_composite_join_both_columns").await;
    let db = ctx.db.get().await?;

    let schema = Schema::new();
    create_table_without_asserts(
        &db,
        &schema.create_table_from_entity(composite_parent::Entity),
    )
    .await?;
    create_table_without_asserts(
        &db,
        &schema.create_table_from_entity(composite_child::Entity),
    )
    .await?;

    // Every parent shares its `region` with another and its `code` with
    // another, so a join that dropped either column would match more than one.
    for (region, code, name) in [(1, 1, "one-one"), (1, 2, "one-two"), (2, 1, "two-one")] {
        composite_parent::ActiveModel {
            region: set(region),
            code: set(code),
            name: set(name),
        }
        .insert(&db)
        .await?;
    }
    for (id, parent_region, parent_code, label) in [(1, 1, 1, "a"), (2, 1, 2, "b"), (3, 2, 1, "c")]
    {
        composite_child::ActiveModel {
            id: set(id),
            parent_region: set(parent_region),
            parent_code: set(parent_code),
            label: set(label),
        }
        .insert(&db)
        .await?;
    }

    let joined = composite_child::Entity::find().find_also_related(composite_parent::Entity);

    // Both pairs are in the ON clause, ANDed.
    assert!(
        joined
            .as_query()
            .to_string()
            .contains(r#"ON "composite_child"."parent_region" = "composite_parent"."region" AND "#)
    );
    assert!(
        joined
            .as_query()
            .to_string()
            .contains(r#""composite_child"."parent_code" = "composite_parent"."code""#)
    );

    let rows = joined
        .order_by_asc(composite_child::Column::Id)
        .all(&db)
        .await?;

    let names: Vec<Option<String>> = rows
        .iter()
        .map(|(_, parent)| parent.as_ref().map(|p| p.name.clone()))
        .collect();

    // One row per child — a join short of a column would repeat children — and
    // each paired with the parent matching on both columns.
    assert_eq!(rows.len(), 3);
    assert_eq!(
        names,
        [
            Some("one-one".to_owned()),
            Some("one-two".to_owned()),
            Some("two-one".to_owned()),
        ]
    );

    drop(db);
    ctx.delete().await;

    Ok(())
}

// ---------------------------------------------------------------------------
// Relation definitions, builders, links and foreign keys
// ---------------------------------------------------------------------------

/// `Identity` is `Iden` but not `Display`; render one for comparison.
fn ident(i: &Identity) -> String {
    pgorm::Iden::to_string(i)
}

/// `ForeignKeyAction` is not `PartialEq`; compare its debug rendering.
fn action(a: &Option<pgorm_query::ForeignKeyAction>) -> String {
    format!("{a:?}")
}

// [spec:pgorm:req:entity.relation+1/test]    `RelationType` has exactly two
// variants; `belongs_to` starts a `HasOne` builder with `is_owner = false`;
// `has_one` / `has_many` derive theirs from `R::to().rev()` with
// `is_owner = true`; and `Related::to` / `via` / `find_related` behave as stated
#[test]
fn relation_trait_and_ownership_direction() {
    use pgorm::{RelationBuilder, RelationType};
    use pgorm_query::QueryBuilder;

    // `RelationType` is a closed two-variant enum: belongs-to is expressed as
    // ownership direction, not a third type. This match is exhaustive.
    for rel_type in [RelationType::HasOne, RelationType::HasMany] {
        match rel_type {
            RelationType::HasOne | RelationType::HasMany => {}
        }
    }
    assert_ne!(RelationType::HasOne, RelationType::HasMany);

    // `RelationTrait::def` maps each variant of the entity's Relation enum to a
    // definition. `baker::Relation::Bakery` is a `belongs_to`.
    let def = baker::Relation::Bakery.def();
    assert_eq!(def.rel_type, RelationType::HasOne);
    assert!(!def.is_owner, "belongs_to is the non-owning side");
    assert_eq!(def.from_tbl, baker::Entity.table_ref().into());
    assert_eq!(def.to_tbl, bakery::Entity.table_ref().into());

    // `belongs_to` starts a HasOne builder with `is_owner = false`, whatever the
    // columns end up being.
    let built: RelationDef = RelationDef::from(
        baker::Entity::belongs_to(bakery::Entity)
            .columns(baker::Column::BakeryId, bakery::Column::Id),
    );
    assert_eq!(built.rel_type, RelationType::HasOne);
    assert!(!built.is_owner);

    // `has_many` requires `R: Related<Self>` and derives its builder from the
    // related entity's definition reversed, with `is_owner = true`.
    let owner: RelationDef = bakery::Entity::has_many(baker::Entity).into();
    let reversed = <baker::Entity as Related<bakery::Entity>>::to().rev();
    assert_eq!(owner.rel_type, RelationType::HasMany);
    assert!(owner.is_owner, "has_many is the owning side");
    assert_eq!(owner.from_tbl, reversed.from_tbl);
    assert_eq!(owner.to_tbl, reversed.to_tbl);
    assert_eq!(
        ident(&owner.columns.from_identity()),
        ident(&reversed.columns.from_identity())
    );
    assert_eq!(
        ident(&owner.columns.to_identity()),
        ident(&reversed.columns.to_identity())
    );
    // Note: `RelationBuilder::from_rel` takes only the tables and columns from
    // the reversed definition; the FK actions the related side declared
    // (`on_update = Cascade`, `on_delete = SetNull`) do NOT carry over.
    assert_eq!(action(&reversed.on_delete), "Some(SetNull)");
    assert_eq!(action(&owner.on_delete), "None");
    assert_eq!(action(&owner.on_update), "None");

    // `has_one` is the same derivation with a HasOne type.
    let one: RelationDef = bakery::Entity::has_one(baker::Entity).into();
    assert_eq!(one.rel_type, RelationType::HasOne);
    assert!(one.is_owner);

    // `Related::via` defaults to `None`...
    assert!(<bakery::Entity as Related<baker::Entity>>::via().is_none());
    // ...and `find_related` inner-joins `to()` onto a fresh `Select<R>`.
    assert_eq!(
        <bakery::Entity as Related<baker::Entity>>::find_related()
            .as_query()
            .to_string(),
        [
            r#"SELECT "baker"."id", "baker"."name", "baker"."contact_details", "baker"."bakery_id""#,
            r#"FROM "baker""#,
            r#"INNER JOIN "bakery" ON "bakery"."id" = "baker"."bakery_id""#,
        ]
        .join(" ")
    );

    // `Some(via)` denotes a junction-table hop, joined in reverse alongside `to()`.
    assert!(<baker::Entity as Related<cake::Entity>>::via().is_some());
    assert_eq!(
        <baker::Entity as Related<cake::Entity>>::find_related()
            .as_query()
            .to_string(),
        [
            r#"SELECT "cake"."id", "cake"."name", "cake"."price", "cake"."bakery_id", "cake"."gluten_free", "cake"."serial""#,
            r#"FROM "cake""#,
            r#"INNER JOIN "cakes_bakers" ON "cakes_bakers"."cake_id" = "cake"."id""#,
            r#"INNER JOIN "baker" ON "baker"."id" = "cakes_bakers"."baker_id""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:def:entity.relation.def+3/test]    the `RelationDef` record and its
// combinators: `rev()` swaps from/to, negates `is_owner`, clears `fk_name` and
// keeps everything else; `from_alias` re-points the source table; `on_condition`
// replaces any existing custom condition; `condition_type` picks AND vs OR.
// Also `Identity`'s arity encoding and the `IntoIdentity` conversions
#[test]
fn relation_def_record_and_combinators() {
    use pgorm::{Identity, IntoIdentity, RelationType};
    use pgorm_query::{
        Alias, ConditionType, FromItem, IntoCondition, NamedTable, QueryBuilder, TableName,
    };

    // Start from a definition carrying every optional attribute.
    let def: RelationDef = RelationDef::from(
        baker::Entity::belongs_to(bakery::Entity)
            .columns(baker::Column::BakeryId, bakery::Column::Id)
            .on_delete(pgorm_query::ForeignKeyAction::Cascade)
            .on_update(pgorm_query::ForeignKeyAction::Restrict)
            .fk_name("fk-custom")
            .condition_type(ConditionType::Any),
    );
    assert_eq!(def.rel_type, RelationType::HasOne);
    assert_eq!(def.from_tbl, baker::Entity.table_ref().into());
    assert_eq!(def.to_tbl, bakery::Entity.table_ref().into());
    assert_eq!(ident(&def.columns.from_identity()), "bakery_id");
    assert_eq!(ident(&def.columns.to_identity()), "id");
    assert!(!def.is_owner);
    assert_eq!(action(&def.on_delete), "Some(Cascade)");
    assert_eq!(action(&def.on_update), "Some(Restrict)");
    assert_eq!(def.fk_name.as_deref(), Some("fk-custom"));
    assert_eq!(def.condition_type, ConditionType::Any);
    assert!(def.on_condition.is_none());

    // `rev()` swaps the tables and the columns, negates `is_owner`, drops the
    // fk name, and keeps rel_type / actions / condition_type.
    let rev = def.rev();
    assert_eq!(rev.from_tbl, bakery::Entity.table_ref().into());
    assert_eq!(rev.to_tbl, baker::Entity.table_ref().into());
    assert_eq!(ident(&rev.columns.from_identity()), "id");
    assert_eq!(ident(&rev.columns.to_identity()), "bakery_id");
    assert!(rev.is_owner, "rev() negates is_owner");
    assert_eq!(rev.fk_name, None, "rev() clears fk_name");
    assert_eq!(rev.rel_type, RelationType::HasOne);
    assert_eq!(action(&rev.on_delete), "Some(Cascade)");
    assert_eq!(action(&rev.on_update), "Some(Restrict)");
    assert_eq!(rev.condition_type, ConditionType::Any);
    // Reversing twice is the identity on tables, columns and ownership.
    let round = rev.rev();
    assert_eq!(round.from_tbl, baker::Entity.table_ref().into());
    assert_eq!(ident(&round.columns.from_identity()), "bakery_id");
    assert!(!round.is_owner);

    // `from_alias` re-points `from_tbl` at an alias, which is what makes a
    // self-join disambiguation possible.
    let aliased = baker::Relation::Bakery.def().from_alias(Alias::new("b2"));
    assert!(matches!(
        &aliased.from_tbl,
        FromItem::Table(NamedTable { name: TableName::Table(table), alias: Some(alias) })
            if table.to_string() == "baker" && alias.to_string() == "b2"
    ));
    // Joining through the re-pointed definition qualifies the source side of
    // the ON clause with the alias instead of the real table name.
    assert_eq!(
        baker::Entity::find()
            .join(JoinType::LeftJoin, aliased)
            .as_query()
            .to_string(),
        [
            r#"SELECT "baker"."id", "baker"."name", "baker"."contact_details", "baker"."bakery_id""#,
            r#"FROM "baker""#,
            r#"LEFT JOIN "bakery" ON "b2"."bakery_id" = "bakery"."id""#,
        ]
        .join(" ")
    );

    // `on_condition` sets a custom join condition, and calling it again replaces
    // rather than accumulates: the second predicate is the only one present.
    let once = baker::Relation::Bakery.def().on_condition(|_l, r| {
        Expr::col((r, bakery::Column::Id))
            .gt(10i32)
            .into_condition()
    });
    let sql_once = bakery::Entity::find()
        .join(JoinType::LeftJoin, once)
        .as_query()
        .to_string();
    assert!(sql_once.contains(r#""bakery"."id" > 10"#));

    let twice = baker::Relation::Bakery
        .def()
        .on_condition(|_l, r| {
            Expr::col((r, bakery::Column::Id))
                .gt(10i32)
                .into_condition()
        })
        .on_condition(|_l, r| {
            Expr::col((r, bakery::Column::Id))
                .lt(99i32)
                .into_condition()
        });
    let sql_twice = bakery::Entity::find()
        .join(JoinType::LeftJoin, twice)
        .as_query()
        .to_string();
    assert!(sql_twice.contains(r#""bakery"."id" < 99"#));
    assert!(
        !sql_twice.contains(r#""bakery"."id" > 10"#),
        "on_condition replaces, it does not accumulate"
    );

    // `condition_type` decides how the generated column equality and the custom
    // condition combine: All -> AND, Any -> OR.
    let on_clause = |ct: ConditionType| {
        let rel = baker::Relation::Bakery
            .def()
            .condition_type(ct)
            .on_condition(|_l, r| {
                Expr::col((r, bakery::Column::Id))
                    .gt(10i32)
                    .into_condition()
            });
        bakery::Entity::find()
            .join(JoinType::LeftJoin, rel)
            .as_query()
            .to_string()
            .split_once(" ON ")
            .expect("an ON clause")
            .1
            .to_owned()
    };
    assert_eq!(
        on_clause(ConditionType::All),
        r#""baker"."bakery_id" = "bakery"."id" AND "bakery"."id" > 10"#
    );
    assert_eq!(
        on_clause(ConditionType::Any),
        r#""baker"."bakery_id" = "bakery"."id" OR "bakery"."id" > 10"#
    );

    // `Identity` encodes column-set arity, and `IntoIdentity` reaches it from
    // `&str`, `String`, any `IdenStatic`, and tuples.
    assert!(matches!("code".into_identity(), Identity::Unary(_)));
    assert!(matches!(
        "code".to_owned().into_identity(),
        Identity::Unary(_)
    ));
    assert!(matches!(
        bakery::Column::Id.into_identity(),
        Identity::Unary(_)
    ));
    assert!(matches!(
        (bakery::Column::Id, bakery::Column::Name).into_identity(),
        Identity::Binary(..)
    ));
    assert!(matches!(
        (
            bakery::Column::Id,
            bakery::Column::Name,
            bakery::Column::ProfitMargin
        )
            .into_identity(),
        Identity::Ternary(..)
    ));
    assert!(matches!(
        (
            cake::Column::Id,
            cake::Column::Name,
            cake::Column::Price,
            cake::Column::BakeryId
        )
            .into_identity(),
        Identity::Many(v) if v.len() == 4
    ));

    // An `Identity` iterates its components in order.
    let components: Vec<String> = (bakery::Column::Id, bakery::Column::Name)
        .into_identity()
        .into_iter()
        .map(|i| i.to_string())
        .collect();
    assert_eq!(components, ["id", "name"]);

    // A composite foreign key is declared a pair at a time, and each side
    // projects back out as an `Identity` of the matching arity.
    let composite: RelationDef = RelationDef::from(
        cakes_bakers::Entity::belongs_to(cakes_bakers::Entity)
            .columns(cakes_bakers::Column::CakeId, cakes_bakers::Column::CakeId)
            .and_columns(cakes_bakers::Column::BakerId, cakes_bakers::Column::BakerId),
    );
    assert!(matches!(
        composite.columns.from_identity(),
        Identity::Binary(..)
    ));
    assert!(matches!(
        composite.columns.to_identity(),
        Identity::Binary(..)
    ));
}

// [spec:pgorm:req:entity.relation.builder+1/test]    the `belongs_to` path starts
// with no columns and both must be supplied; the `has_one` / `has_many` path
// pre-fills them from the reversed related definition; the optional attributes
// are settable; and `condition_type` defaults to `All`
#[test]
fn relation_builder_accumulates_a_definition() {
    use pgorm::{Identity, RelationBuilder, RelationType};
    use pgorm_query::ConditionType;

    // A `belongs_to` builder with both columns converts cleanly.
    let def: RelationDef = RelationDef::from(
        baker::Entity::belongs_to(bakery::Entity)
            .columns(baker::Column::BakeryId, bakery::Column::Id),
    );
    assert!(matches!(def.columns.from_identity(), Identity::Unary(_)));
    assert!(matches!(def.columns.to_identity(), Identity::Unary(_)));
    // Nothing optional was set, and `condition_type` defaults to All.
    assert_eq!(action(&def.on_delete), "None");
    assert_eq!(action(&def.on_update), "None");
    assert_eq!(def.fk_name, None);
    assert!(def.on_condition.is_none());
    assert_eq!(def.condition_type, ConditionType::All);

    // The `has_many` path pre-fills both columns, so no `.from` / `.to` needed.
    let prefilled: RelationDef = bakery::Entity::has_many(baker::Entity).into();
    assert_eq!(ident(&prefilled.columns.from_identity()), "id");
    assert_eq!(ident(&prefilled.columns.to_identity()), "bakery_id");
    assert_eq!(prefilled.condition_type, ConditionType::All);

    // Every optional attribute is settable through the builder.
    let full: RelationDef = RelationDef::from(
        baker::Entity::belongs_to(bakery::Entity)
            .columns(baker::Column::BakeryId, bakery::Column::Id)
            .on_delete(pgorm_query::ForeignKeyAction::SetNull)
            .on_update(pgorm_query::ForeignKeyAction::Cascade)
            .on_condition(|_l, _r| pgorm_query::Condition::all())
            .fk_name("fk-baker-bakery_id")
            .condition_type(ConditionType::Any),
    );
    assert_eq!(action(&full.on_delete), "Some(SetNull)");
    assert_eq!(action(&full.on_update), "Some(Cascade)");
    assert_eq!(full.fk_name.as_deref(), Some("fk-baker-bakery_id"));
    assert!(full.on_condition.is_some());
    assert_eq!(full.condition_type, ConditionType::Any);
    assert_eq!(full.rel_type, RelationType::HasOne);
}

// [spec:pgorm:def:entity.relation.def+3/test]    a set of join columns is a
// list of pairs, so both sides always name the same number of columns however
// the definition is built, reversed or extended
#[test]
fn column_pairs_keep_the_two_sides_equal() {
    use pgorm::ColumnPairs;
    use pgorm_query::Alias;

    let count = |identity: &Identity| identity.clone().into_iter().count();
    let balanced = |columns: &ColumnPairs| {
        assert_eq!(columns.arity(), count(&columns.from_identity()));
        assert_eq!(columns.arity(), count(&columns.to_identity()));
    };

    // Hand-built, extended one pair at a time, up to the arity where `Identity`
    // stops having a dedicated variant.
    let mut columns = ColumnPairs::new(Alias::new("a1"), Alias::new("b1"));
    for n in 2..=5 {
        balanced(&columns);
        columns = columns.and(Alias::new(format!("a{n}")), Alias::new(format!("b{n}")));
    }
    balanced(&columns);
    balanced(&columns.clone().rev());

    // ...and built through the builder, where the composite case is the one a
    // zip of two independently-supplied column lists could truncate.
    let composite: RelationDef = RelationDef::from(
        cakes_bakers::Entity::belongs_to(cakes_bakers::Entity)
            .columns(cakes_bakers::Column::CakeId, cakes_bakers::Column::CakeId)
            .and_columns(cakes_bakers::Column::BakerId, cakes_bakers::Column::BakerId),
    );
    balanced(&composite.columns);
    assert_eq!(composite.columns.arity(), 2);
    balanced(&composite.rev().columns);
}

// [spec:pgorm:req:entity.relation.fk+3/test]    `From<RelationDef>` for both
// `ForeignKeyCreateStatement` and `TableForeignKey` maps every column pair,
// applies the `on_delete` / `on_update` actions, takes the constraint name from
// `fk_name` when set and otherwise derives `fk-{from_table}-{from_cols}`, and
// reduces schema-qualified table references to bare tables
#[test]
fn relation_def_converts_to_foreign_key_forms() {
    use pgorm_query::{
        Alias, ConditionType, ForeignKeyCreateStatement, FromItem, IntoIden, QueryBuilder, Table,
        TableForeignKey, TableName,
    };

    let alter =
        |fk: &mut TableForeignKey| Table::alter(baker::Entity).add_foreign_key(fk).to_string();

    // With an explicit `fk_name`, that name is used verbatim.
    let named: RelationDef = RelationDef::from(
        baker::Entity::belongs_to(bakery::Entity)
            .columns(baker::Column::BakeryId, bakery::Column::Id)
            .fk_name("fk-custom-name"),
    );
    let stmt: ForeignKeyCreateStatement = named.into();
    assert_eq!(
        stmt.to_string(),
        [
            r#"ALTER TABLE "baker" ADD CONSTRAINT "fk-custom-name""#,
            r#"FOREIGN KEY ("bakery_id") REFERENCES "bakery" ("id")"#,
        ]
        .join(" ")
    );

    // Without one, the name is derived as `fk-{from_table}-{from_cols}`, and the
    // FK actions are applied.
    let derived: RelationDef = RelationDef::from(
        baker::Entity::belongs_to(bakery::Entity)
            .columns(baker::Column::BakeryId, bakery::Column::Id)
            .on_delete(pgorm_query::ForeignKeyAction::SetNull)
            .on_update(pgorm_query::ForeignKeyAction::Cascade),
    );
    let stmt: ForeignKeyCreateStatement = derived.into();
    assert_eq!(
        stmt.to_string(),
        [
            r#"ALTER TABLE "baker" ADD CONSTRAINT "fk-baker-bakery_id""#,
            r#"FOREIGN KEY ("bakery_id") REFERENCES "bakery" ("id")"#,
            r#"ON DELETE SET NULL ON UPDATE CASCADE"#,
        ]
        .join(" ")
    );

    // Composite keys map every component, and the derived name joins the source
    // columns with '-'.
    let composite: RelationDef = RelationDef::from(
        cakes_bakers::Entity::belongs_to(cakes_bakers::Entity)
            .columns(cakes_bakers::Column::CakeId, cakes_bakers::Column::CakeId)
            .and_columns(cakes_bakers::Column::BakerId, cakes_bakers::Column::BakerId),
    );
    let stmt: ForeignKeyCreateStatement = composite.into();
    assert_eq!(
        stmt.to_string(),
        [
            r#"ALTER TABLE "cakes_bakers" ADD CONSTRAINT "fk-cakes_bakers-cake_id-baker_id""#,
            r#"FOREIGN KEY ("cake_id", "baker_id") REFERENCES "cakes_bakers" ("cake_id", "baker_id")"#,
        ]
        .join(" ")
    );

    // The `TableForeignKey` conversion carries the same mapping.
    let mut fk: TableForeignKey = RelationDef::from(
        baker::Entity::belongs_to(bakery::Entity)
            .columns(baker::Column::BakeryId, bakery::Column::Id)
            .on_delete(pgorm_query::ForeignKeyAction::Cascade),
    )
    .into();
    assert_eq!(
        alter(&mut fk),
        [
            r#"ALTER TABLE "baker" ADD CONSTRAINT "fk-baker-bakery_id""#,
            r#"FOREIGN KEY ("bakery_id") REFERENCES "bakery" ("id") ON DELETE CASCADE"#,
        ]
        .join(" ")
    );

    // Schema information is reduced away: a schema-qualified `FromItem` becomes
    // a bare table on both sides, and the derived name uses the bare name too.
    let qualified = RelationDef {
        rel_type: RelationType::HasOne,
        from_tbl: FromItem::from(TableName::SchemaTable(
            Alias::new("warehouse").into_iden(),
            Alias::new("child").into_iden(),
        )),
        to_tbl: FromItem::from(TableName::SchemaTable(
            Alias::new("warehouse").into_iden(),
            Alias::new("parent").into_iden(),
        )),
        columns: ColumnPairs::new(Alias::new("parent_id"), Alias::new("id")),
        is_owner: false,
        on_delete: None,
        on_update: None,
        on_condition: None,
        fk_name: None,
        condition_type: ConditionType::All,
    };
    let stmt: ForeignKeyCreateStatement = qualified.into();
    assert_eq!(
        stmt.to_string(),
        [
            r#"ALTER TABLE "child" ADD CONSTRAINT "fk-child-parent_id""#,
            r#"FOREIGN KEY ("parent_id") REFERENCES "parent" ("id")"#,
        ]
        .join(" ")
    );
}

/// A two-hop link with a custom condition on the outer hop, to pin the
/// `on_condition` augmentation `find_linked` applies per hop.
struct FilteredBakerCakes;

impl Linked for FilteredBakerCakes {
    // `IntoCondition` has to be in scope for the `on_condition` closures below.
    type FromEntity = baker::Entity;
    type ToEntity = cake::Entity;

    fn link(&self) -> Vec<RelationDef> {
        use pgorm_query::IntoCondition;

        vec![
            cakes_bakers::Relation::Baker
                .def()
                .rev()
                .on_condition(|_l, r| {
                    Expr::col((r, cakes_bakers::Column::CakeId))
                        .gt(10i32)
                        .into_condition()
                }),
            cakes_bakers::Relation::Cake.def(),
        ]
    }
}

// [spec:pgorm:req:entity.relation.linked/test]    `find_linked` walks the chain
// in reverse, aliasing each hop's source table `r0`, `r1`, ... and inner-joining
// it to the previous alias while the innermost hop joins the unaliased target
// table; each hop's `on_condition` closure is added to that hop's join
// condition; and `ModelTrait::find_linked` scopes the result on `r{len - 1}`
#[test]
fn linked_chain_aliasing_and_conditions() {
    use pgorm_query::QueryBuilder;

    // `link()` is the ordered chain from FromEntity to ToEntity.
    assert_eq!(baker::BakedForCustomer.link().len(), 5);

    // `find_linked` reverses it: the last hop's source becomes `r0` joined to
    // the unaliased target table, then r1 joins r0, and so on.
    assert_eq!(
        baker::BakedForCustomer.find_linked().as_query().to_string(),
        [
            r#"SELECT "customer"."id", "customer"."name", "customer"."notes""#,
            r#"FROM "customer""#,
            r#"INNER JOIN "order" AS "r0" ON "r0"."customer_id" = "customer"."id""#,
            r#"INNER JOIN "lineitem" AS "r1" ON "r1"."order_id" = "r0"."id""#,
            r#"INNER JOIN "cake" AS "r2" ON "r2"."id" = "r1"."cake_id""#,
            r#"INNER JOIN "cakes_bakers" AS "r3" ON "r3"."cake_id" = "r2"."id""#,
            r#"INNER JOIN "baker" AS "r4" ON "r4"."id" = "r3"."baker_id""#,
        ]
        .join(" ")
    );

    // `ModelTrait::find_linked` scopes that to one instance by filtering on the
    // final alias, `r{len - 1}` = r4 for a five-hop chain.
    let bob = baker::Model {
        id: 1,
        name: "Baker Bob".to_owned(),
        contact_details: serde_json::json!({}),
        bakery_id: Some(1),
    };
    assert!(
        bob.find_linked(baker::BakedForCustomer)
            .as_query()
            .to_string()
            .ends_with(r#"WHERE "r4"."id" = 1"#)
    );

    // A hop's `on_condition` is added to that hop's join condition, alongside
    // the generated column equality rather than replacing it.
    let sql = FilteredBakerCakes.find_linked().as_query().to_string();
    assert_eq!(
        sql,
        [
            r#"SELECT "cake"."id", "cake"."name", "cake"."price", "cake"."bakery_id", "cake"."gluten_free", "cake"."serial""#,
            r#"FROM "cake""#,
            r#"INNER JOIN "cakes_bakers" AS "r0" ON "r0"."cake_id" = "cake"."id""#,
            r#"INNER JOIN "baker" AS "r1" ON "r1"."id" = "r0"."baker_id" AND "r0"."cake_id" > 10"#,
        ]
        .join(" ")
    );
}
