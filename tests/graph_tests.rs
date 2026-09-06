//! Live coverage for the N-ary relational read.
//!
//! Three decoded sources with same-named columns and absent middles, a
//! required slot decoding without `Option`, the same table entering twice
//! under an alias with a call-site `ON`, a junction folded in by
//! `related_maybe`, a `via` hop re-tightened by a required slot, and the
//! terminals landing on the ordinary selector machinery.
//!
//! Run locally:
//! `DATABASE_URL="postgres://postgres:postgres@localhost:54329" cargo test --test graph_tests`

#![allow(unused_imports, dead_code)]

pub mod common;

use futures::TryStreamExt;
use pgorm::tests_cfg::{cake, cake_filling, filling, fruit, vendor};
use pgorm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, Error, PaginatorTrait,
    QueryFilter, QueryOrder, RelationTrait, alias,
};
use pgorm_query::{Expr, IntoCondition};
use pretty_assertions::assert_eq;
use std::num::NonZeroU64;

pub use common::TestContext;

/// The fixture: three cakes, one of which has no junction row at all, one
/// whose filling has no vendor, and a cake with two fruits.
const SCHEMA: &str = r#"
    CREATE TABLE "cake" ("id" int PRIMARY KEY, "name" text NOT NULL);
    CREATE TABLE "vendor" ("id" int PRIMARY KEY, "name" text NOT NULL);
    CREATE TABLE "filling" ("id" int PRIMARY KEY, "name" text NOT NULL, "vendor_id" int);
    CREATE TABLE "cake_filling" (
        "cake_id" int, "filling_id" int, PRIMARY KEY ("cake_id", "filling_id")
    );
    CREATE TABLE "fruit" ("id" int PRIMARY KEY, "name" text NOT NULL, "cake_id" int);
    INSERT INTO "cake" VALUES (1, 'Cheesecake'), (2, 'Lonely'), (3, 'Mudcake');
    INSERT INTO "vendor" VALUES (7, 'Sweet Supplies');
    INSERT INTO "filling" VALUES (5, 'Cream', 7), (6, 'Orphanite', NULL);
    INSERT INTO "cake_filling" VALUES (1, 5), (3, 6);
    INSERT INTO "fruit" VALUES (10, 'Cherry', 1), (11, 'Peach', 1);
"#;

async fn schema(db: &DatabaseConnection) -> Result<(), Error> {
    db.batch_execute(SCHEMA).await
}

fn cheesecake() -> cake::Model {
    cake::Model {
        id: 1,
        name: "Cheesecake".to_owned(),
    }
}

fn lonely() -> cake::Model {
    cake::Model {
        id: 2,
        name: "Lonely".to_owned(),
    }
}

fn mudcake() -> cake::Model {
    cake::Model {
        id: 3,
        name: "Mudcake".to_owned(),
    }
}

fn cream() -> filling::Model {
    filling::Model {
        id: 5,
        name: "Cream".to_owned(),
        vendor_id: Some(7),
        ignored_attr: 0,
    }
}

fn orphanite() -> filling::Model {
    filling::Model {
        id: 6,
        name: "Orphanite".to_owned(),
        vendor_id: None,
        ignored_attr: 0,
    }
}

fn sweet_supplies() -> vendor::Model {
    vendor::Model {
        id: 7,
        name: "Sweet Supplies".to_owned(),
    }
}

fn cherry() -> fruit::Model {
    fruit::Model {
        id: 10,
        name: "Cherry".to_owned(),
        cake_id: Some(1),
    }
}

fn peach() -> fruit::Model {
    fruit::Model {
        id: 11,
        name: "Peach".to_owned(),
        cake_id: Some(1),
    }
}

/// A page size written as a literal; the `NonZeroU64` is the check.
fn page_size(size: u64) -> NonZeroU64 {
    NonZeroU64::new(size).expect("page size is non-zero")
}

// [spec:pgorm:def:query.graph/test]    a graph declared as a root plus joined
// sources executes as one statement whose rows carry every declared source
// [spec:pgorm:sem:query.graph.writer/test]    three sources each carrying a
// `name` and an `id` column decode without collision, because each is
// projected under its own `s{i}_` prefix
// [spec:pgorm:sem:query.graph.decode/test]    an unmatched LEFT JOIN reads as
// `None` through the absence witness, at the middle of a chain and at its tail
#[pgorm_macros::test]
async fn graph_three_sources() -> Result<(), Error> {
    let ctx = TestContext::new("graph_three_sources").await;
    let db = ctx.db.get().await?;
    schema(&db).await?;

    let rows = cake::Entity::graph()
        .via(cake_filling::Relation::Cake.def().rev())
        .join_maybe::<filling::Entity>(cake_filling::Relation::Filling.def())
        .join_maybe::<vendor::Entity>(filling::Relation::Vendor.def())
        .order_by_asc(cake::Column::Id)
        .all(&db)
        .await?;

    assert_eq!(
        rows,
        [
            (cheesecake(), Some(cream()), Some(sweet_supplies())),
            // No junction row at all: the middle is absent, and so is the tail.
            (lonely(), None, None),
            // A present middle whose own foreign key is NULL.
            (mudcake(), Some(orphanite()), None),
        ]
    );

    drop(db);
    ctx.delete().await;
    Ok(())
}

// [spec:pgorm:sem:query.graph.slots/test]    a required slot joins INNER and
// decodes as a bare `Model` — "absent" is not a value the caller unwraps —
// while the same relation as an optional slot keeps the unmatched roots
#[pgorm_macros::test]
async fn graph_required_slot() -> Result<(), Error> {
    let ctx = TestContext::new("graph_required_slot").await;
    let db = ctx.db.get().await?;
    schema(&db).await?;

    let required: Vec<(cake::Model, fruit::Model)> = cake::Entity::graph()
        .join_one::<fruit::Entity>(cake::Relation::Fruit.def())
        .order_by_asc(cake::Column::Id)
        .order_by_asc(fruit::Column::Id)
        .all(&db)
        .await?;

    assert_eq!(
        required,
        [(cheesecake(), cherry()), (cheesecake(), peach())]
    );

    let optional: Vec<(cake::Model, Option<fruit::Model>)> = cake::Entity::graph()
        .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
        .order_by_asc(cake::Column::Id)
        .order_by_asc(fruit::Column::Id)
        .all(&db)
        .await?;

    assert_eq!(
        optional,
        [
            (cheesecake(), Some(cherry())),
            (cheesecake(), Some(peach())),
            (lonely(), None),
            (mudcake(), None),
        ]
    );

    drop(db);
    ctx.delete().await;
    Ok(())
}

// [spec:pgorm:req:query.graph.aliases/test]    the same table enters the graph
// twice under a caller-bound alias, which is the slot's identifier in the ON
// clause and in the projection alike, so both copies decode independently
#[pgorm_macros::test]
async fn graph_same_table_twice() -> Result<(), Error> {
    let ctx = TestContext::new("graph_same_table_twice").await;
    let db = ctx.db.get().await?;
    schema(&db).await?;

    let rows = cake::Entity::graph()
        .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
        .join_maybe_as::<fruit::Entity>(
            cake::Relation::Fruit.def().on_condition(|_left, right| {
                Expr::col((right, fruit::Column::Name))
                    .like("Ch%")
                    .into_condition()
            }),
            alias("starts_with_ch"),
        )
        .order_by_asc(cake::Column::Id)
        .order_by_asc(fruit::Column::Id)
        .all(&db)
        .await?;

    assert_eq!(
        rows,
        [
            // The aliased copy is narrowed by the call-site ON, so the second
            // slot repeats Cherry against both of the first slot's rows.
            (cheesecake(), Some(cherry()), Some(cherry())),
            (cheesecake(), Some(peach()), Some(cherry())),
            (lonely(), None, None),
            (mudcake(), None, None),
        ]
    );

    drop(db);
    ctx.delete().await;
    Ok(())
}

// [spec:pgorm:req:query.graph.aliases/test]    `join_maybe_filtered` puts the
// call-site predicate in ON, where unmatched roots survive decoding `None`,
// rather than in WHERE, where the join silently tightens to INNER
#[pgorm_macros::test]
async fn graph_filtered_join_keeps_unmatched_roots() -> Result<(), Error> {
    let ctx = TestContext::new("graph_filtered_join").await;
    let db = ctx.db.get().await?;
    schema(&db).await?;

    let in_on = cake::Entity::graph()
        .join_maybe_filtered::<fruit::Entity, _>(cake::Relation::Fruit.def(), |_left, right| {
            Expr::col((right, fruit::Column::Name))
                .like("Ch%")
                .into_condition()
        })
        .order_by_asc(cake::Column::Id)
        .all(&db)
        .await?;

    assert_eq!(
        in_on,
        [
            (cheesecake(), Some(cherry())),
            (lonely(), None),
            (mudcake(), None),
        ]
    );

    // The same predicate through `filter` lands in WHERE, and the NULLs of an
    // unmatched row fail it.
    let in_where = cake::Entity::graph()
        .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
        .filter(fruit::Column::Name.like("Ch%"))
        .order_by_asc(cake::Column::Id)
        .all(&db)
        .await?;

    assert_eq!(in_where, [(cheesecake(), Some(cherry()))]);

    drop(db);
    ctx.delete().await;
    Ok(())
}

// [spec:pgorm:sem:query.graph.slots/test]    `related_maybe` folds the whole
// described path in — junction hop included — and a required slot joined
// through a `via` hop re-tightens the chain to INNER semantics end to end
#[pgorm_macros::test]
async fn graph_related_and_via() -> Result<(), Error> {
    let ctx = TestContext::new("graph_related_and_via").await;
    let db = ctx.db.get().await?;
    schema(&db).await?;

    let folded = cake::Entity::graph()
        .related_maybe::<filling::Entity>()
        .order_by_asc(cake::Column::Id)
        .all(&db)
        .await?;

    assert_eq!(
        folded,
        [
            (cheesecake(), Some(cream())),
            (lonely(), None),
            (mudcake(), Some(orphanite())),
        ]
    );

    // `via` alone is LEFT, so it cannot erase a root; the required slot behind
    // it does, because its ON references the middle's columns.
    let retightened: Vec<(cake::Model, filling::Model)> = cake::Entity::graph()
        .via(cake_filling::Relation::Cake.def().rev())
        .join_one::<filling::Entity>(cake_filling::Relation::Filling.def())
        .order_by_asc(cake::Column::Id)
        .all(&db)
        .await?;

    assert_eq!(
        retightened,
        [(cheesecake(), cream()), (mudcake(), orphanite())]
    );

    // A via-then-join_one chain that also re-tightens at its tail: only the
    // filling with a vendor survives.
    let tail: Vec<(cake::Model, vendor::Model)> = cake::Entity::graph()
        .via(cake_filling::Relation::Cake.def().rev())
        .via(cake_filling::Relation::Filling.def())
        .join_one::<vendor::Entity>(filling::Relation::Vendor.def())
        .order_by_asc(cake::Column::Id)
        .all(&db)
        .await?;

    assert_eq!(tail, [(cheesecake(), sweet_supplies())]);

    drop(db);
    ctx.delete().await;
    Ok(())
}

/// A fruit whose `name` is NULL, which `fruit::Model` has no `Option` for.
const NULLABLE_SCHEMA: &str = r#"
    CREATE TABLE "cake" ("id" int PRIMARY KEY, "name" text NOT NULL);
    CREATE TABLE "fruit" ("id" int PRIMARY KEY, "name" text, "cake_id" int);
    INSERT INTO "cake" VALUES (1, 'Cheesecake');
    INSERT INTO "fruit" VALUES (10, NULL, 1);
"#;

// [spec:pgorm:sem:query.graph.decode/test]    a decode failure of a *present*
// row propagates rather than being read as absence — on a required slot,
// which never consults the witness, and on an optional one, whose witness
// columns are not all NULL
#[pgorm_macros::test]
async fn graph_decode_failure_is_not_absence() -> Result<(), Error> {
    let ctx = TestContext::new("graph_decode_failure").await;
    let db = ctx.db.get().await?;
    db.batch_execute(NULLABLE_SCHEMA).await?;

    let required = cake::Entity::graph()
        .join_one::<fruit::Entity>(cake::Relation::Fruit.def())
        .all(&db)
        .await;
    assert!(
        required.is_err(),
        "a NULL in a required slot's non-Option field is an error: {required:?}"
    );

    let optional = cake::Entity::graph()
        .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
        .all(&db)
        .await;
    assert!(
        optional.is_err(),
        "a matched row that fails to decode is not `None`: {optional:?}"
    );

    drop(db);
    ctx.delete().await;
    Ok(())
}

// [spec:pgorm:sem:query.graph.terminals/test]    `all`, `one_opt`, `stream`
// and `PaginatorTrait` all run through one `Selector<GraphRow<E, S>>`, and
// page boundaries fall between rows rather than between root models
#[pgorm_macros::test]
async fn graph_terminals() -> Result<(), Error> {
    let ctx = TestContext::new("graph_terminals").await;
    let db = ctx.db.get().await?;
    schema(&db).await?;

    let graph = || {
        cake::Entity::graph()
            .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
            .order_by_asc(cake::Column::Id)
            .order_by_asc(fruit::Column::Id)
    };

    let all = graph().all(&db).await?;
    assert_eq!(all.len(), 4);

    // `one_opt` injects LIMIT 1 and answers the first-row question.
    assert_eq!(
        graph().one_opt(&db).await?,
        Some((cheesecake(), Some(cherry())))
    );
    assert_eq!(
        graph().filter(cake::Column::Id.gt(99)).one_opt(&db).await?,
        None
    );

    // The stream decodes lazily per item and yields the same rows.
    let streamed: Vec<_> = graph().stream(&db).await?.try_collect().await?;
    assert_eq!(streamed, all);

    // Pagination cuts between rows: Cheesecake's two fruit rows fill page 0
    // whole, and the fruitless cakes make up page 1.
    let paginator = graph().paginate(&db, page_size(2));
    assert_eq!(paginator.fetch_page(0).await?, all[..2]);
    assert_eq!(paginator.fetch_page(1).await?, all[2..]);
    assert_eq!(paginator.num_items().await?, 4);

    assert_eq!(graph().count(&db).await?, 4);

    // A slotless graph decodes as a bare model, not a one-tuple.
    let bare: Vec<cake::Model> = cake::Entity::graph()
        .order_by_asc(cake::Column::Id)
        .all(&db)
        .await?;
    assert_eq!(bare, [cheesecake(), lonely(), mudcake()]);

    drop(db);
    ctx.delete().await;
    Ok(())
}
