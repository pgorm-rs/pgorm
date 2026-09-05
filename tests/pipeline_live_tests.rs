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
use common::bakery_chain::{customer::Column as C, order::Column as O};
pub use common::{TestContext, bakery_chain::*, setup::*};
use pgorm::pipeline::{
    AliasName, ExprOps, IntoSource, JoinSide, Pipeline, alias, by, col, count_rows, row_number,
    sort_by, sum,
};
use pgorm::{ConnectionTrait, Schema, entity::*, set};
use pretty_assertions::assert_eq;
use rust_decimal::Decimal;

/// Names this suite's pipelines introduce: bound once here, referred to by
/// value everywhere below.
const SPENT: AliasName = alias("spent");
const ORDER_COUNT: AliasName = alias("order_count");
const RN: AliasName = alias("rn");
const CUSTOMER_ID: AliasName = alias("customer_id");
const RUNNING: AliasName = alias("running");
const MANAGER: AliasName = alias("manager");
const PARENT: AliasName = alias("parent");
const ID: AliasName = alias("id");
const NAME: AliasName = alias("name");
const BODY: AliasName = alias("body");

/// A table that refers to itself: every employee but the founder reports to
/// another row of this same table.
mod employee {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "employee")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        pub name: String,
        pub manager_id: Option<i32>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[pgorm(belongs_to = "Entity", from = "Column::ManagerId", to = "Column::Id")]
        Manager,
    }

    impl ActiveModelBehavior for ActiveModel {}
}

/// The same shape with the reference left unset on the root row, so a left
/// join has a `NULL` to carry.
mod message {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "message")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        pub body: String,
        pub parent_id: Option<i32>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[pgorm(belongs_to = "Entity", from = "Column::ParentId", to = "Column::Id")]
        Parent,
    }

    impl ActiveModelBehavior for ActiveModel {}
}

struct Seeded {
    alice: i32,
    bob: i32,
    cleo: i32,
}

/// Three customers with 3, 2 and 1 orders: Alice spends 60, Bob 50, Cleo 5.
async fn seed(db: &impl ConnectionTrait) -> Seeded {
    let bakery = bakery::ActiveModel {
        name: set("SeaSide Bakery"),
        profit_margin: set(10.4),
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
            name: set(name),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("could not insert customer");
        for total in totals {
            order::ActiveModel {
                bakery_id: set(bakery.id),
                customer_id: set(customer.id),
                total: set(rust_dec(total)),
                placed_at: set(Utc::now().naive_utc()),
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
    Pipeline::from(order::Entity)
        .group(O::CustomerId)
        .aggregate((sum(O::Total).as_(SPENT), count_rows().as_(ORDER_COUNT)))
}

// [spec:pgorm:req:pipeline.surface+3/test]    HAVING placement with a bound
// threshold: the group filter runs against grouped sums, not rows
#[pgorm_macros::test]
async fn having_filters_groups_with_bound_param() {
    let ctx = TestContext::new("pipeline_having_filters_groups").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();
    let seeded = seed(&db).await;

    let pipeline = spending()
        .filter_with(|binder| SPENT.gt(binder.bind(rust_dec(40.0))))
        .sort(SPENT.desc());
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

// [spec:pgorm:req:pipeline.surface+3/test]    filter-after-window nests the
// windowed stage in a CTE and filters outside it, with the rank bound
#[pgorm_macros::test]
async fn window_rank_filter_nests_through_cte() {
    let ctx = TestContext::new("pipeline_window_rank_cte").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();
    let seeded = seed(&db).await;

    let pipeline = Pipeline::from(order::Entity)
        .window(
            row_number().as_(RN),
            by(O::CustomerId).sort_by(O::Total.desc()),
        )
        .filter_with(|binder| RN.lte(binder.bind(2_i64)))
        .select((O::CustomerId, O::Total, RN))
        .sort((O::CustomerId, RN));
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

// [spec:pgorm:req:pipeline.surface+3/test]    explicit join condition against
// live rows, with the customer name bound
#[pgorm_macros::test]
async fn join_on_explicit_condition_binds_params() {
    let ctx = TestContext::new("pipeline_join_explicit_condition").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();
    seed(&db).await;

    let rows: Vec<(Decimal, String)> = Pipeline::from(order::Entity)
        .join(JoinSide::Inner, customer::Entity, O::CustomerId.eq(C::Id))
        .filter_with(|binder| C::Name.eq(binder.bind("Alice")))
        .select((O::Total, C::Name))
        .sort(O::Total)
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

// [spec:pgorm:req:pipeline.surface+3/test]    an explicit ROWS frame computes
// a running sum per partition on live rows
#[pgorm_macros::test]
async fn rows_frame_computes_running_sum() {
    let ctx = TestContext::new("pipeline_rows_frame_running_sum").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();
    let seeded = seed(&db).await;

    let rows: Vec<(i32, Decimal, Decimal)> = Pipeline::from(order::Entity)
        .window(
            sum(O::Total).as_(RUNNING),
            by(O::CustomerId).sort_by(O::Total).rows(None, Some(0)),
        )
        .filter_with(|binder| O::CustomerId.eq(binder.bind(seeded.alice)))
        .select((O::CustomerId, O::Total, RUNNING))
        .sort(O::Total)
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

// [spec:pgorm:req:pipeline.surface+3/test]    fn(Pipeline) -> Pipeline scopes
// compose, each binding its own parameters, and the placeholders stay aligned
#[pgorm_macros::test]
async fn composed_scopes_bind_params_in_order() {
    let ctx = TestContext::new("pipeline_composed_scopes").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();
    let seeded = seed(&db).await;

    fn spent_over(pipeline: Pipeline, threshold: Decimal) -> Pipeline {
        pipeline.filter_with(move |binder| SPENT.gt(binder.bind(threshold)))
    }
    fn fewer_orders_than(pipeline: Pipeline, count: i64) -> Pipeline {
        pipeline.filter_with(move |binder| ORDER_COUNT.lt(binder.bind(count)))
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

    let customers: Vec<customer::Model> = Pipeline::from(customer::Entity)
        .filter_with(|binder| C::Name.ne(binder.bind("Cleo")))
        .sort(C::Name)
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

    let bob: customer::Model = Pipeline::from(customer::Entity)
        .filter_with(|binder| C::Name.eq(binder.bind("Bob")))
        .one(&db)
        .await
        .unwrap();
    assert_eq!(bob.name, "Bob");

    let nobody: Option<customer::Model> = Pipeline::from(customer::Entity)
        .filter_with(|binder| C::Name.eq(binder.bind("Zed")))
        .one_opt(&db)
        .await
        .unwrap();
    assert!(nobody.is_none());

    ctx.delete().await;
}

// [spec:pgorm:req:pipeline.compose/test]    a union of two filtered
// pipelines, one bound param each: the values interleave with their $N
#[pgorm_macros::test]
async fn union_of_filtered_pipelines_binds_both_params() {
    let ctx = TestContext::new("pipeline_union_two_filtered").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();
    seed(&db).await;

    let small = Pipeline::from(order::Entity)
        .filter_with(|binder| O::Total.lt(binder.bind(rust_dec(6.0))))
        .select(O::Total);
    let pipeline = Pipeline::from(order::Entity)
        .filter_with(|binder| O::Total.gt(binder.bind(rust_dec(25.0))))
        .select(O::Total)
        .append(small)
        .sort(O::Total);
    let (sql, values) = pipeline.clone().into_sql().unwrap();
    assert!(sql.contains("UNION ALL"), "{sql}");
    assert_eq!(values.0.len(), 2);

    let rows: Vec<(Decimal,)> = pipeline.into_tuple().unwrap().all(&db).await.unwrap();
    assert_eq!(
        rows,
        vec![(rust_dec(5.00),), (rust_dec(25.50),), (rust_dec(30.00),),]
    );

    ctx.delete().await;
}

// [spec:pgorm:req:pipeline.compose/test]    top spenders joined back to
// their customers: params bound in the consumer and in the embedded pipeline
#[pgorm_macros::test]
async fn top_spenders_join_binds_across_pipelines() {
    let ctx = TestContext::new("pipeline_top_spenders_join").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();
    seed(&db).await;

    let spenders = spending().filter_with(|binder| SPENT.gt(binder.bind(rust_dec(40.0))));
    let pipeline = Pipeline::from(customer::Entity)
        .filter_with(|binder| C::Name.ne(binder.bind("Zed")))
        .join(JoinSide::Inner, spenders, C::Id.eq(CUSTOMER_ID))
        .select((C::Name, SPENT))
        .sort(SPENT.desc());
    let (sql, values) = pipeline.clone().into_sql().unwrap();
    assert!(parsed_select(&sql).with_clause.is_some());
    assert_eq!(values.0.len(), 2);

    let rows: Vec<(String, Decimal)> = pipeline.into_tuple().unwrap().all(&db).await.unwrap();
    assert_eq!(
        rows,
        vec![
            ("Alice".to_owned(), rust_dec(60.00)),
            ("Bob".to_owned(), rust_dec(50.00)),
        ]
    );

    ctx.delete().await;
}

// [spec:pgorm:req:pipeline.compose/test]    remove drops one matching row
// per row of the removed pipeline, with a param on each side
#[pgorm_macros::test]
async fn remove_pipeline_subtracts_matching_rows() {
    let ctx = TestContext::new("pipeline_remove_matching").await;
    create_tables(&ctx.db).await.unwrap();
    let db = ctx.db.get().await.unwrap();
    let seeded = seed(&db).await;

    let big = Pipeline::from(order::Entity)
        .filter_with(|binder| O::Total.gt(binder.bind(rust_dec(24.0))))
        .select(O::CustomerId);
    let pipeline = Pipeline::from(order::Entity)
        .filter_with(|binder| O::Total.gt(binder.bind(rust_dec(9.0))))
        .select(O::CustomerId)
        .remove(big)
        .sort(CUSTOMER_ID);
    let (sql, values) = pipeline.clone().into_sql().unwrap();
    assert!(sql.contains("EXCEPT ALL"), "{sql}");
    assert_eq!(values.0.len(), 2);

    let rows: Vec<(i32,)> = pipeline.into_tuple().unwrap().all(&db).await.unwrap();
    assert_eq!(rows, vec![(seeded.alice,), (seeded.alice,)]);

    ctx.delete().await;
}

async fn create_entity_table<E: EntityTrait>(db: &impl ConnectionTrait, entity: E) {
    let stmt = Schema::new().create_table_from_entity(entity);
    create_table_without_asserts(db, &stmt)
        .await
        .expect("could not create table");
}

/// Ada founded the company; Grace and Linus report to her, Alan to Grace.
async fn seed_employees(db: &impl ConnectionTrait) {
    let ada = employee::ActiveModel {
        name: set("Ada"),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("could not insert employee");
    let grace = employee::ActiveModel {
        name: set("Grace"),
        manager_id: set(ada.id),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("could not insert employee");
    for (name, manager) in [("Linus", ada.id), ("Alan", grace.id)] {
        employee::ActiveModel {
            name: set(name),
            manager_id: set(manager),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("could not insert employee");
    }
}

// [spec:pgorm:sem:pipeline.self-join/test]    the classic employee-manager
// query on live rows: one table, two names, both sides decoded
#[pgorm_macros::test]
async fn self_join_decodes_employee_and_manager() {
    let ctx = TestContext::new("pipeline_self_join_manager").await;
    let db = ctx.db.get().await.unwrap();
    create_entity_table(&db, employee::Entity).await;
    seed_employees(&db).await;

    let pipeline = Pipeline::from(employee::Entity)
        .join(
            JoinSide::Inner,
            employee::Entity.named(MANAGER),
            employee::Column::ManagerId.eq(col(MANAGER, ID)),
        )
        .select((
            employee::Column::Name,
            col(MANAGER, NAME).as_(alias("boss")),
        ))
        .sort(employee::Column::Name);
    let (sql, _) = pipeline.clone().into_sql().unwrap();
    assert!(sql.contains("employee AS manager"), "{sql}");
    assert!(parsed_select(&sql).with_clause.is_none(), "{sql}");

    let rows: Vec<(String, String)> = pipeline.into_tuple().unwrap().all(&db).await.unwrap();
    assert_eq!(
        rows,
        vec![
            ("Alan".to_owned(), "Grace".to_owned()),
            ("Grace".to_owned(), "Ada".to_owned()),
            ("Linus".to_owned(), "Ada".to_owned()),
        ]
    );

    ctx.delete().await;
}

// [spec:pgorm:sem:pipeline.self-join/test]    a left self-join over a nullable
// reference keeps the root row, its parent decoding as None
#[pgorm_macros::test]
async fn left_self_join_decodes_a_null_parent() {
    let ctx = TestContext::new("pipeline_self_join_parent").await;
    let db = ctx.db.get().await.unwrap();
    create_entity_table(&db, message::Entity).await;

    let root = message::ActiveModel {
        body: set("root"),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let reply = message::ActiveModel {
        body: set("reply"),
        parent_id: set(root.id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    message::ActiveModel {
        body: set("nested"),
        parent_id: set(reply.id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let pipeline = Pipeline::from(message::Entity)
        .join(
            JoinSide::Left,
            message::Entity.named(PARENT),
            message::Column::ParentId.eq(col(PARENT, ID)),
        )
        .sort(message::Column::Id)
        .select((
            message::Column::Body,
            col(PARENT, BODY).as_(alias("parent_body")),
        ));
    let (sql, _) = pipeline.clone().into_sql().unwrap();
    assert!(sql.contains("LEFT OUTER JOIN message AS parent"), "{sql}");

    let rows: Vec<(String, Option<String>)> =
        pipeline.into_tuple().unwrap().all(&db).await.unwrap();
    assert_eq!(
        rows,
        vec![
            ("root".to_owned(), None),
            ("reply".to_owned(), Some("root".to_owned())),
            ("nested".to_owned(), Some("reply".to_owned())),
        ]
    );

    ctx.delete().await;
}

// [spec:pgorm:sem:pipeline.self-join/test]    the same query through an
// embedded pipeline: the far side is renamed before it crosses
#[pgorm_macros::test]
async fn embedded_self_join_renames_before_crossing() {
    let ctx = TestContext::new("pipeline_self_join_embedded").await;
    let db = ctx.db.get().await.unwrap();
    create_entity_table(&db, employee::Entity).await;
    seed_employees(&db).await;

    let boss = alias("boss");
    let managers = Pipeline::from(employee::Entity)
        .filter_with(|binder| employee::Column::Name.ne(binder.bind("Grace")))
        .select((
            employee::Column::Id.as_(alias("manager_pk")),
            employee::Column::Name.as_(boss),
        ));
    let pipeline = Pipeline::from(employee::Entity)
        .join(
            JoinSide::Inner,
            managers.named(MANAGER),
            employee::Column::ManagerId.eq(col(MANAGER, alias("manager_pk"))),
        )
        .select((employee::Column::Name, col(MANAGER, boss)))
        .sort(employee::Column::Name);
    let (sql, values) = pipeline.clone().into_sql().unwrap();
    assert!(parsed_select(&sql).with_clause.is_some(), "{sql}");
    assert_eq!(values.0.len(), 1);

    let rows: Vec<(String, String)> = pipeline.into_tuple().unwrap().all(&db).await.unwrap();
    assert_eq!(
        rows,
        vec![
            ("Grace".to_owned(), "Ada".to_owned()),
            ("Linus".to_owned(), "Ada".to_owned()),
        ]
    );

    ctx.delete().await;
}
