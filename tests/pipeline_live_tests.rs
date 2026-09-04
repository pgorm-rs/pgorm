#![allow(unused_imports, dead_code)]

//! The pipeline API against a live server, with bound parameters.
//!
//! The unit suite holds every emitted string to a golden form and the
//! pg_query oracle; what it cannot prove is that the clause placement means
//! what it says — that a `HAVING` filters groups, that a filter-after-window
//! really applies outside the CTE, that `$N` values arrive in the right
//! slots. Those are semantics, so they are asserted against PostgreSQL with
//! decoded rows.
//!
//! Run the test locally:
//! DATABASE_URL=postgres://postgres:postgres@127.0.0.1:54329 cargo test --test pipeline_live_tests

pub mod common;

pub use chrono::offset::Utc;
pub use common::{TestContext, bakery_chain::*, setup::*};
use pgorm::pipeline::{
    Frame, JoinSide, Pipeline, WindowDef, col, count_rows, out, row_number, sum,
};
use pgorm::{ActiveValue::Set, ConnectionTrait, entity::*};
use pretty_assertions::assert_eq;
use rust_decimal::Decimal;

struct Seeded {
    alice: i32,
    bob: i32,
    cleo: i32,
}

/// Three customers with 3, 2 and 1 orders: Alice spends 60, Bob 50, Cleo 5.
async fn seed(db: &impl ConnectionTrait) -> Seeded {
    let bakery = bakery::ActiveModel {
        name: Set("SeaSide Bakery".to_owned()),
        profit_margin: Set(10.4),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("could not insert bakery");

    let mut ids = Vec::new();
    for (name, totals) in [
        ("Alice", vec![10.00, 20.00, 30.00]),
        ("Bob", vec![25.50, 24.50]),
        ("Cleo", vec![5.00]),
    ] {
        let customer = customer::ActiveModel {
            name: Set(name.to_owned()),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("could not insert customer");
        for total in totals {
            order::ActiveModel {
                bakery_id: Set(bakery.id),
                customer_id: Set(customer.id),
                total: Set(rust_dec(total)),
                placed_at: Set(Utc::now().naive_utc()),
                ..Default::default()
            }
            .insert(db)
            .await
            .expect("could not insert order");
        }
        ids.push(customer.id);
    }
    Seeded {
        alice: ids[0],
        bob: ids[1],
        cleo: ids[2],
    }
}

fn parsed_select(sql: &str) -> pg_query::protobuf::SelectStmt {
    let parsed = pg_query::parse(sql).expect("grammar accepts");
    let node = parsed.protobuf.stmts[0]
        .stmt
        .as_ref()
        .and_then(|stmt| stmt.node.as_ref())
        .expect("statement present");
    match node {
        pg_query::NodeEnum::SelectStmt(select) => (**select).clone(),
        other => panic!("expected SelectStmt, got {other:?}"),
    }
}

fn spending() -> Pipeline {
    Pipeline::from(order::Entity).aggregate_by(|_| {
        (
            vec![col(order::Entity, order::Column::CustomerId)],
            vec![
                sum(col(order::Entity, order::Column::Total)).aliased("spent"),
                count_rows().aliased("order_count"),
            ],
        )
    })
}

// [spec:pgorm:req:pipeline.surface/test]    HAVING placement with a bound
// threshold: the group filter runs against grouped sums, not rows
#[pgorm_macros::test]
async fn having_filters_groups_with_bound_param() {
    let ctx = TestContext::new("pipeline_having_filters_groups").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();
    let seeded = seed(&db).await;

    let pipeline = spending()
        .filter(|binder| out("spent").gt(binder.bind(rust_dec(40.0))))
        .sort(|_| vec![out("spent").desc()]);
    let (sql, _) = pipeline.clone().into_sql().unwrap();
    assert!(parsed_select(&sql).having_clause.is_some());

    let rows: Vec<(i32, Decimal, i64)> = pipeline.into_tuple().unwrap().all(&db).await.unwrap();
    assert_eq!(
        rows,
        vec![
            (seeded.alice, rust_dec(60.00), 3),
            (seeded.bob, rust_dec(50.00), 2),
        ]
    );

    ctx.delete().await;
}

// [spec:pgorm:req:pipeline.surface/test]    filter-after-window nests the
// windowed stage in a CTE and filters outside it, with the rank bound
#[pgorm_macros::test]
async fn window_rank_filter_nests_through_cte() {
    let ctx = TestContext::new("pipeline_window_rank_cte").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();
    let seeded = seed(&db).await;

    let pipeline = Pipeline::from(order::Entity)
        .window(|_| {
            WindowDef::derive(vec![row_number().aliased("rn")])
                .partition_by(vec![col(order::Entity, order::Column::CustomerId)])
                .sorted(vec![col(order::Entity, order::Column::Total).desc()])
        })
        .filter(|binder| out("rn").lte(binder.bind(2_i64)))
        .select(|_| {
            vec![
                col(order::Entity, order::Column::CustomerId),
                col(order::Entity, order::Column::Total),
                out("rn"),
            ]
        })
        .sort(|_| vec![col(order::Entity, order::Column::CustomerId), out("rn")]);
    let (sql, _) = pipeline.clone().into_sql().unwrap();
    let parsed = parsed_select(&sql);
    assert!(parsed.with_clause.is_some());
    assert!(parsed.where_clause.is_some());

    let rows: Vec<(i32, Decimal, i64)> = pipeline.into_tuple().unwrap().all(&db).await.unwrap();
    assert_eq!(
        rows,
        vec![
            (seeded.alice, rust_dec(30.00), 1),
            (seeded.alice, rust_dec(20.00), 2),
            (seeded.bob, rust_dec(25.50), 1),
            (seeded.bob, rust_dec(24.50), 2),
            (seeded.cleo, rust_dec(5.00), 1),
        ]
    );

    ctx.delete().await;
}

// [spec:pgorm:req:pipeline.surface/test]    explicit join condition against
// live rows, with the customer name bound
#[pgorm_macros::test]
async fn join_on_explicit_condition_binds_params() {
    let ctx = TestContext::new("pipeline_join_explicit_condition").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();
    seed(&db).await;

    let rows: Vec<(Decimal, String)> = Pipeline::from(order::Entity)
        .join(JoinSide::Inner, customer::Entity, |_| {
            col(order::Entity, order::Column::CustomerId)
                .eq(col(customer::Entity, customer::Column::Id))
        })
        .filter(|binder| col(customer::Entity, customer::Column::Name).eq(binder.bind("Alice")))
        .select(|_| {
            vec![
                col(order::Entity, order::Column::Total),
                col(customer::Entity, customer::Column::Name),
            ]
        })
        .sort(|_| vec![col(order::Entity, order::Column::Total)])
        .into_tuple()
        .unwrap()
        .all(&db)
        .await
        .unwrap();
    assert_eq!(
        rows,
        vec![
            (rust_dec(10.00), "Alice".to_owned()),
            (rust_dec(20.00), "Alice".to_owned()),
            (rust_dec(30.00), "Alice".to_owned()),
        ]
    );

    ctx.delete().await;
}

// [spec:pgorm:req:pipeline.surface/test]    an explicit ROWS frame computes a
// running sum per partition on live rows
#[pgorm_macros::test]
async fn rows_frame_computes_running_sum() {
    let ctx = TestContext::new("pipeline_rows_frame_running_sum").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();
    let seeded = seed(&db).await;

    let rows: Vec<(i32, Decimal, Decimal)> = Pipeline::from(order::Entity)
        .window(|_| {
            WindowDef::derive(vec![
                sum(col(order::Entity, order::Column::Total)).aliased("running"),
            ])
            .partition_by(vec![col(order::Entity, order::Column::CustomerId)])
            .sorted(vec![col(order::Entity, order::Column::Total)])
            .frame(Frame::rows(None, Some(0)))
        })
        .filter(|binder| {
            col(order::Entity, order::Column::CustomerId).eq(binder.bind(seeded.alice))
        })
        .select(|_| {
            vec![
                col(order::Entity, order::Column::CustomerId),
                col(order::Entity, order::Column::Total),
                out("running"),
            ]
        })
        .sort(|_| vec![col(order::Entity, order::Column::Total)])
        .into_tuple()
        .unwrap()
        .all(&db)
        .await
        .unwrap();
    assert_eq!(
        rows,
        vec![
            (seeded.alice, rust_dec(10.00), rust_dec(10.00)),
            (seeded.alice, rust_dec(20.00), rust_dec(30.00)),
            (seeded.alice, rust_dec(30.00), rust_dec(60.00)),
        ]
    );

    ctx.delete().await;
}

// [spec:pgorm:req:pipeline.surface/test]    fn(Pipeline) -> Pipeline scopes
// compose, each binding its own parameters, and the placeholders stay aligned
#[pgorm_macros::test]
async fn composed_scopes_bind_params_in_order() {
    let ctx = TestContext::new("pipeline_composed_scopes").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();
    let seeded = seed(&db).await;

    fn spent_over(pipeline: Pipeline, threshold: Decimal) -> Pipeline {
        pipeline.filter(move |binder| out("spent").gt(binder.bind(threshold)))
    }
    fn fewer_orders_than(pipeline: Pipeline, count: i64) -> Pipeline {
        pipeline.filter(move |binder| out("order_count").lt(binder.bind(count)))
    }

    let pipeline = fewer_orders_than(spent_over(spending(), rust_dec(40.0)), 3);
    let (sql, values) = pipeline.clone().into_sql().unwrap();
    assert!(sql.contains("$1") && sql.contains("$2"), "{sql}");
    assert_eq!(values.0.len(), 2);

    let rows: Vec<(i32, Decimal, i64)> = pipeline.into_tuple().unwrap().all(&db).await.unwrap();
    assert_eq!(rows, vec![(seeded.bob, rust_dec(50.00), 2)]);

    ctx.delete().await;
}

// [spec:pgorm:sem:pipeline.terminal/test]    the model terminal goes straight
// from a pipeline to entity models on a connection
#[pgorm_macros::test]
async fn terminal_decodes_entity_models() {
    let ctx = TestContext::new("pipeline_terminal_models").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();
    seed(&db).await;

    let customers: Vec<customer::Model> = Pipeline::from_entity::<customer::Entity>()
        .filter(|binder| col(customer::Entity, customer::Column::Name).ne(binder.bind("Cleo")))
        .sort(|_| vec![col(customer::Entity, customer::Column::Name)])
        .all(&db)
        .await
        .unwrap();
    assert_eq!(
        customers
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Alice", "Bob"]
    );

    let bob: customer::Model = Pipeline::from_entity::<customer::Entity>()
        .filter(|binder| col(customer::Entity, customer::Column::Name).eq(binder.bind("Bob")))
        .one(&db)
        .await
        .unwrap();
    assert_eq!(bob.name, "Bob");

    let nobody: Option<customer::Model> = Pipeline::from_entity::<customer::Entity>()
        .filter(|binder| col(customer::Entity, customer::Column::Name).eq(binder.bind("Zed")))
        .one_opt(&db)
        .await
        .unwrap();
    assert!(nobody.is_none());

    ctx.delete().await;
}
