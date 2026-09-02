#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, bakery_chain::*, setup::*};
use futures::FutureExt;
use pgorm::pgorm_query::{Alias, Expr};
use pgorm::{ActiveValue::Set, DatabaseConnection, DbErr, RuntimeErr, Schema, entity::*, query::*};

// [spec:pgorm:req:query.loader/test]    `load_one` over a `Vec<M>`, taking a
// bare entity through `EntityOrSelect`, returning `Vec<Option<R::Model>>`
// positionally aligned with the input, and rejecting a `HasMany` relation
#[pgorm_macros::test]
async fn loader_load_one() -> Result<(), DbErr> {
    let ctx = TestContext::new("loader_test_load_one").await;
    create_tables(&ctx.db).await?;
    let db = &ctx.db.get().await?;

    let bakery_0 = insert_bakery(db, "SeaSide Bakery").await?;

    let baker_1 = insert_baker(db, "Baker 1", bakery_0.id).await?;
    let baker_2 = insert_baker(db, "Baker 2", bakery_0.id).await?;
    let baker_3 = baker::ActiveModel {
        name: Set("Baker 3".to_owned()),
        contact_details: Set(serde_json::json!({})),
        bakery_id: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await?;

    let bakers = baker::Entity::find().all(db).await?;
    let bakeries = bakers.load_one(bakery::Entity, db).await?;

    assert_eq!(bakers, [baker_1, baker_2, baker_3]);
    assert_eq!(bakeries, [Some(bakery_0.clone()), Some(bakery_0), None]);

    // has many find, should use load_many instead
    let bakeries = bakery::Entity::find().all(db).await?;
    let bakers = bakeries.load_one(baker::Entity, db).await;

    assert_eq!(
        bakers,
        Err(DbErr::Query(RuntimeErr::Internal(
            "Relation is HasMany instead of HasOne".to_string()
        )))
    );

    Ok(())
}

// [spec:pgorm:req:query.loader/test]    `load_many` returning `Vec<Vec<..>>`
// aligned with the input, driven from both a bare entity and a pre-filtered
// `Select<R>`
// [spec:pgorm:sem:query.loader.regroup/test]    a bucket per input key in
// result order, an empty `Vec` for an input nothing matched, and a clone of
// the same model for two inputs sharing a key
#[pgorm_macros::test]
async fn loader_load_many() -> Result<(), DbErr> {
    let ctx = TestContext::new("loader_test_load_many").await;
    create_tables(&ctx.db).await?;
    let db = &ctx.db.get().await?;

    let bakery_1 = insert_bakery(db, "SeaSide Bakery").await?;
    let bakery_2 = insert_bakery(db, "Offshore Bakery").await?;
    let bakery_3 = insert_bakery(db, "Rocky Bakery").await?;

    let baker_1 = insert_baker(db, "Baker 1", bakery_1.id).await?;
    let baker_2 = insert_baker(db, "Baker 2", bakery_1.id).await?;

    let baker_3 = insert_baker(db, "John", bakery_2.id).await?;
    let baker_4 = insert_baker(db, "Baker 4", bakery_2.id).await?;

    let bakeries = bakery::Entity::find().all(db).await?;
    let bakers = bakeries.load_many(baker::Entity, db).await?;

    assert_eq!(
        bakeries,
        [bakery_1.clone(), bakery_2.clone(), bakery_3.clone()]
    );
    assert_eq!(
        bakers,
        [
            vec![baker_1.clone(), baker_2.clone()],
            vec![baker_3.clone(), baker_4.clone()],
            vec![]
        ]
    );

    // load bakers again but with additional condition

    let bakers = bakeries
        .load_many(
            baker::Entity::find().filter(baker::Column::Name.like("Baker%")),
            db,
        )
        .await?;

    assert_eq!(
        bakers,
        [
            vec![baker_1.clone(), baker_2.clone()],
            vec![baker_4.clone()],
            vec![]
        ]
    );

    // now, start from baker

    let bakers = baker::Entity::find().all(db).await?;
    let bakeries = bakers.load_one(bakery::Entity::find(), db).await?;

    // note that two bakers share the same bakery
    assert_eq!(bakers, [baker_1, baker_2, baker_3, baker_4]);
    assert_eq!(
        bakeries,
        [
            Some(bakery_1.clone()),
            Some(bakery_1),
            Some(bakery_2.clone()),
            Some(bakery_2)
        ]
    );

    Ok(())
}

#[pgorm_macros::test]
async fn loader_load_many_multi() -> Result<(), DbErr> {
    let ctx = TestContext::new("loader_test_load_many_multi").await;
    create_tables(&ctx.db).await?;
    let db = &ctx.db.get().await?;

    let bakery_1 = insert_bakery(db, "SeaSide Bakery").await?;
    let bakery_2 = insert_bakery(db, "Offshore Bakery").await?;

    let baker_1 = insert_baker(db, "John", bakery_1.id).await?;
    let baker_2 = insert_baker(db, "Jane", bakery_1.id).await?;
    let baker_3 = insert_baker(db, "Peter", bakery_2.id).await?;

    let cake_1 = insert_cake(db, "Cheesecake", Some(bakery_1.id)).await?;
    let cake_2 = insert_cake(db, "Chocolate", Some(bakery_2.id)).await?;
    let cake_3 = insert_cake(db, "Chiffon", Some(bakery_2.id)).await?;
    let _cake_4 = insert_cake(db, "Apple Pie", None).await?; // no one makes apple pie

    let bakeries = bakery::Entity::find().all(db).await?;
    let bakers = bakeries.load_many(baker::Entity, db).await?;
    let cakes = bakeries.load_many(cake::Entity, db).await?;

    assert_eq!(bakeries, [bakery_1, bakery_2]);
    assert_eq!(bakers, [vec![baker_1, baker_2], vec![baker_3]]);
    assert_eq!(cakes, [vec![cake_1], vec![cake_2, cake_3]]);

    Ok(())
}

// [spec:pgorm:req:query.loader/test]    `load_many_to_many` across a junction,
// from a bare entity and from a pre-filtered `Select<R>`
// [spec:pgorm:sem:query.loader.many-to-many/test]    the junction is resolved
// first and the targets second: a shared target is cloned into every
// referencing input, and a foreign key whose target row the caller's `Select`
// filtered away is silently dropped from that input's list
#[pgorm_macros::test]
async fn loader_load_many_to_many() -> Result<(), DbErr> {
    let ctx = TestContext::new("loader_test_load_many_to_many").await;
    create_tables(&ctx.db).await?;
    let db = &ctx.db.get().await?;

    let bakery_1 = insert_bakery(db, "SeaSide Bakery").await?;

    let baker_1 = insert_baker(db, "Jane", bakery_1.id).await?;
    let baker_2 = insert_baker(db, "Peter", bakery_1.id).await?;
    let baker_3 = insert_baker(db, "Fred", bakery_1.id).await?; // does not make cake

    let cake_1 = insert_cake(db, "Cheesecake", None).await?;
    let cake_2 = insert_cake(db, "Coffee", None).await?;
    let cake_3 = insert_cake(db, "Chiffon", None).await?;
    let cake_4 = insert_cake(db, "Apple Pie", None).await?; // no one makes apple pie

    insert_cake_baker(db, baker_1.id, cake_1.id).await?;
    insert_cake_baker(db, baker_1.id, cake_2.id).await?;
    insert_cake_baker(db, baker_2.id, cake_2.id).await?;
    insert_cake_baker(db, baker_2.id, cake_3.id).await?;

    let bakers = baker::Entity::find().all(db).await?;
    let cakes = bakers
        .load_many_to_many(cake::Entity, cakes_bakers::Entity, db)
        .await?;

    assert_eq!(bakers, [baker_1.clone(), baker_2.clone(), baker_3.clone()]);
    assert_eq!(
        cakes,
        [
            vec![cake_1.clone(), cake_2.clone()],
            vec![cake_2.clone(), cake_3.clone()],
            vec![]
        ]
    );

    // same, but apply restrictions on cakes

    let cakes = bakers
        .load_many_to_many(
            cake::Entity::find().filter(cake::Column::Name.like("Ch%")),
            cakes_bakers::Entity,
            db,
        )
        .await?;
    assert_eq!(cakes, [vec![cake_1.clone()], vec![cake_3.clone()], vec![]]);

    // now, start again from cakes

    let cakes = cake::Entity::find().all(db).await?;
    let bakers = cakes
        .load_many_to_many(baker::Entity, cakes_bakers::Entity, db)
        .await?;

    assert_eq!(cakes, [cake_1, cake_2, cake_3, cake_4]);
    assert_eq!(
        bakers,
        [
            vec![baker_1.clone()],
            vec![baker_1.clone(), baker_2.clone()],
            vec![baker_2.clone()],
            vec![]
        ]
    );

    Ok(())
}

pub async fn insert_bakery(db: &DatabaseConnection, name: &str) -> Result<bakery::Model, DbErr> {
    bakery::ActiveModel {
        name: Set(name.to_owned()),
        profit_margin: Set(1.0),
        ..Default::default()
    }
    .insert(db)
    .await
}

pub async fn insert_baker(
    db: &DatabaseConnection,
    name: &str,
    bakery_id: i32,
) -> Result<baker::Model, DbErr> {
    baker::ActiveModel {
        name: Set(name.to_owned()),
        contact_details: Set(serde_json::json!({})),
        bakery_id: Set(Some(bakery_id)),
        ..Default::default()
    }
    .insert(db)
    .await
}

pub async fn insert_cake(
    db: &DatabaseConnection,
    name: &str,
    bakery_id: Option<i32>,
) -> Result<cake::Model, DbErr> {
    cake::ActiveModel {
        name: Set(name.to_owned()),
        price: Set(rust_decimal::Decimal::ONE),
        gluten_free: Set(false),
        bakery_id: Set(bakery_id),
        ..Default::default()
    }
    .insert(db)
    .await
}

pub async fn insert_cake_baker(
    db: &DatabaseConnection,
    baker_id: i32,
    cake_id: i32,
) -> Result<cakes_bakers::Model, DbErr> {
    cakes_bakers::ActiveModel {
        cake_id: Set(cake_id),
        baker_id: Set(baker_id),
    }
    .insert(db)
    .await
}

// A fixture purpose-built for the loader's key-mapping edge cases: it lives in
// an explicit schema (exercising the `TableRef::SchemaTable` arm of the key
// predicate), and `owner_id` is deliberately not unique, so a relation declared
// `HasOne` over it can match several rows for one key.
mod ledger {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(schema_name = "public", table_name = "ledger")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        pub owner_id: i32,
        pub label: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// `HasOne` over a non-unique column, so one key can match several rows.
impl Related<ledger::Entity> for bakery::Entity {
    fn to() -> RelationDef {
        bakery::Entity::belongs_to(ledger::Entity)
            .from(bakery::Column::Id)
            .to(ledger::Column::OwnerId)
            .into()
    }
}

/// A junction-mediated relation whose target is `HasMany` — rejected by all
/// three loader entry points, each for its own reason.
impl Related<ledger::Entity> for cake::Entity {
    fn to() -> RelationDef {
        let mut def: RelationDef = cake::Entity::belongs_to(ledger::Entity)
            .from(cake::Column::Id)
            .to(ledger::Column::OwnerId)
            .into();
        def.rel_type = RelationType::HasMany;
        def
    }

    fn via() -> Option<RelationDef> {
        Some(cakes_bakers::Relation::Cake.def().rev())
    }
}

/// The same relation with an aliased target table, which the loader's key
/// predicate cannot qualify.
impl Related<ledger::Entity> for customer::Entity {
    fn to() -> RelationDef {
        let mut def: RelationDef = customer::Entity::belongs_to(ledger::Entity)
            .from(customer::Column::Id)
            .to(ledger::Column::OwnerId)
            .into();
        def.to_tbl = def.to_tbl.alias(Alias::new("l"));
        def
    }
}

/// A self-relation on a composite primary key: every row matches exactly
/// itself, so the batch predicate has to be a tuple `IN` list.
impl Related<cakes_bakers::Entity> for cakes_bakers::Entity {
    fn to() -> RelationDef {
        let mut def: RelationDef = cakes_bakers::Entity::belongs_to(cakes_bakers::Entity)
            .from((cakes_bakers::Column::CakeId, cakes_bakers::Column::BakerId))
            .to((cakes_bakers::Column::CakeId, cakes_bakers::Column::BakerId))
            .into();
        def.rel_type = RelationType::HasMany;
        def
    }
}

fn internal_err<T>(message: &str) -> Result<T, DbErr> {
    Err(DbErr::Query(RuntimeErr::Internal(message.to_owned())))
}

async fn create_ledger_table<C>(db: &C) -> Result<u64, DbErr>
where
    C: ConnectionTrait,
{
    let stmt = Schema::new().create_table_from_entity(ledger::Entity);
    create_table_without_asserts(db, &stmt).await
}

async fn insert_ledger(
    db: &DatabaseConnection,
    owner_id: i32,
    label: &str,
) -> Result<ledger::Model, DbErr> {
    ledger::ActiveModel {
        owner_id: Set(owner_id),
        label: Set(label.to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
}

// [spec:pgorm:req:query.loader/test]    relation shape is validated up front:
// `load_one` rejects a junction, `load_many` rejects a junction and a HasOne
// target, and `load_many_to_many` rejects a missing junction, a non-HasOne
// target and a junction entity that is not the relation's own. The `&[M]` impl
// the `Vec<M>` one delegates to is public API in its own right.
#[pgorm_macros::test]
async fn loader_rejects_wrong_relation_shapes() -> Result<(), DbErr> {
    let ctx = TestContext::new("loader_test_relation_shapes").await;
    create_tables(&ctx.db).await?;
    let db = &ctx.db.get().await?;

    let bakery_1 = insert_bakery(db, "SeaSide Bakery").await?;
    let baker_1 = insert_baker(db, "Jane", bakery_1.id).await?;
    let cake_1 = insert_cake(db, "Cheesecake", None).await?;
    insert_cake_baker(db, baker_1.id, cake_1.id).await?;

    let bakers = baker::Entity::find().all(db).await?;
    let cakes = cake::Entity::find().all(db).await?;

    assert_eq!(
        cakes.load_one(ledger::Entity, db).await,
        internal_err("Relation is ManytoMany instead of HasOne")
    );
    assert_eq!(
        cakes.load_many(ledger::Entity, db).await,
        internal_err("Relation is ManyToMany instead of HasMany")
    );
    assert_eq!(
        bakers.load_many(bakery::Entity, db).await,
        internal_err("Relation is HasOne instead of HasMany")
    );
    assert_eq!(
        bakers
            .load_many_to_many(bakery::Entity, cakes_bakers::Entity, db)
            .await,
        internal_err("Relation is not ManyToMany")
    );
    assert_eq!(
        cakes
            .load_many_to_many(ledger::Entity, cakes_bakers::Entity, db)
            .await,
        internal_err("Relation to is not HasOne")
    );

    // The junction entity is compared against the relation's own junction.
    let wrong_via = bakers
        .load_many_to_many(cake::Entity, bakery::Entity, db)
        .await
        .expect_err("a mismatched junction entity must be rejected");
    match wrong_via {
        DbErr::Query(RuntimeErr::Internal(message)) => assert!(
            message.starts_with("The given via Entity is incorrect"),
            "{message}"
        ),
        other => panic!("unexpected error: {other:?}"),
    }

    // The slice impl carries the actual implementation.
    let slice: &[baker::Model] = bakers.as_slice();
    assert_eq!(slice.load_one(bakery::Entity, db).await?, [Some(bakery_1)]);

    Ok(())
}

// [spec:pgorm:req:query.loader/test]    an empty input short-circuits to an
// empty result without querying: every selector passed here would raise a
// database error if it were ever sent
#[pgorm_macros::test]
async fn loader_empty_input_skips_the_query() -> Result<(), DbErr> {
    let ctx = TestContext::new("loader_test_empty_input").await;
    create_tables(&ctx.db).await?;
    let db = &ctx.db.get().await?;

    let poisoned_bakery = || bakery::Entity::find().filter(Expr::cust("no_such_column IS NULL"));
    let poisoned_baker = || baker::Entity::find().filter(Expr::cust("no_such_column IS NULL"));
    let poisoned_cake = || cake::Entity::find().filter(Expr::cust("no_such_column IS NULL"));

    // The selectors really are unusable against this schema.
    assert!(poisoned_bakery().all(db).await.is_err());

    let no_bakers: Vec<baker::Model> = Vec::new();
    assert_eq!(
        no_bakers.load_one(poisoned_bakery(), db).await?,
        Vec::<Option<bakery::Model>>::new()
    );

    let no_bakeries: &[bakery::Model] = &[];
    assert_eq!(
        no_bakeries.load_many(poisoned_baker(), db).await?,
        Vec::<Vec<baker::Model>>::new()
    );

    let no_bakers_slice: &[baker::Model] = &[];
    assert_eq!(
        no_bakers_slice
            .load_many_to_many(poisoned_cake(), cakes_bakers::Entity, db)
            .await?,
        Vec::<Vec<cake::Model>>::new()
    );

    Ok(())
}

// [spec:pgorm:sem:query.loader.batching/test]    keys are collected in input
// order and become a single IN predicate on the relation's `to_col`: a
// composite key renders as a tuple `IN` list through `in_tuples` (the unary
// `col IN (..)` form is what every other loader test here exercises). The
// predicate is AND-ed onto the caller's `Select`, so a user filter composes
// with it. The rule's note that duplicate keys are repeated rather than
// deduplicated concerns the emitted SQL text, which is not observable through
// this API.
// [spec:pgorm:sem:query.loader.regroup/test]    two inputs sharing a key each
// receive their own clone of that key's bucket
#[pgorm_macros::test]
async fn loader_batches_composite_keys_as_tuples() -> Result<(), DbErr> {
    let ctx = TestContext::new("loader_test_composite_keys").await;
    create_tables(&ctx.db).await?;
    let db = &ctx.db.get().await?;

    let bakery_1 = insert_bakery(db, "SeaSide Bakery").await?;
    let baker_1 = insert_baker(db, "Jane", bakery_1.id).await?;
    let baker_2 = insert_baker(db, "Peter", bakery_1.id).await?;
    let cake_1 = insert_cake(db, "Cheesecake", None).await?;
    let cake_2 = insert_cake(db, "Chiffon", None).await?;

    insert_cake_baker(db, baker_1.id, cake_1.id).await?;
    insert_cake_baker(db, baker_2.id, cake_1.id).await?;
    insert_cake_baker(db, baker_1.id, cake_2.id).await?;

    let rows = cakes_bakers::Entity::find()
        .order_by_asc(cakes_bakers::Column::CakeId)
        .order_by_asc(cakes_bakers::Column::BakerId)
        .all(db)
        .await?;
    assert_eq!(rows.len(), 3);

    // Each composite key matches exactly its own row, positionally.
    let matched = rows.load_many(cakes_bakers::Entity, db).await?;
    assert_eq!(
        matched,
        rows.iter().map(|row| vec![row.clone()]).collect::<Vec<_>>()
    );

    // A caller-supplied filter is AND-ed onto the same tuple predicate.
    let filtered = rows
        .load_many(
            cakes_bakers::Entity::find().filter(cakes_bakers::Column::CakeId.eq(cake_1.id)),
            db,
        )
        .await?;
    assert_eq!(
        filtered,
        rows.iter()
            .map(|row| if row.cake_id == cake_1.id {
                vec![row.clone()]
            } else {
                Vec::new()
            })
            .collect::<Vec<_>>()
    );

    // Repeating an input repeats its bucket, in input order.
    let repeated = vec![rows[2].clone(), rows[0].clone(), rows[0].clone()];
    assert_eq!(
        repeated.load_many(cakes_bakers::Entity, db).await?,
        [
            vec![rows[2].clone()],
            vec![rows[0].clone()],
            vec![rows[0].clone()],
        ]
    );

    Ok(())
}

// [spec:pgorm:sem:query.loader.regroup/test]    `load_one` indexes the returned
// rows into a map keyed on `to_col` in result order, so when a relation
// declared `HasOne` matches several rows for one key the last row wins, an
// unmatched input gets `None`, and inputs sharing a key each get a clone
// [spec:pgorm:req:query.loader.table-ref-limitation/test]    the supported
// `TableRef::SchemaTable` target: its key column is qualified and the load runs
#[pgorm_macros::test]
async fn loader_load_one_keeps_the_last_row() -> Result<(), DbErr> {
    let ctx = TestContext::new("loader_test_last_row_wins").await;
    create_tables(&ctx.db).await?;
    let db = &ctx.db.get().await?;
    create_ledger_table(db).await?;

    let bakery_1 = insert_bakery(db, "SeaSide Bakery").await?;
    let bakery_2 = insert_bakery(db, "Offshore Bakery").await?;

    let first = insert_ledger(db, bakery_1.id, "first").await?;
    let second = insert_ledger(db, bakery_1.id, "second").await?;
    assert!(first.id < second.id);

    let bakeries = vec![bakery_1.clone(), bakery_2, bakery_1];
    let ledgers = bakeries
        .load_one(ledger::Entity::find().order_by_asc(ledger::Column::Id), db)
        .await?;

    assert_eq!(ledgers, [Some(second.clone()), None, Some(second)]);

    Ok(())
}

// [spec:pgorm:req:query.loader.table-ref-limitation/test]    a relation whose
// target resolves to any other `TableRef` variant — an aliased table here —
// cannot have its key column qualified, so the load aborts through
// `unimplemented!` rather than returning an `Err`
#[pgorm_macros::test]
async fn loader_panics_on_unsupported_table_ref() -> Result<(), DbErr> {
    let ctx = TestContext::new("loader_test_table_ref_limit").await;
    create_tables(&ctx.db).await?;
    let db = &ctx.db.get().await?;
    create_ledger_table(db).await?;

    let customers = vec![
        customer::ActiveModel {
            name: Set("Alice".to_owned()),
            ..Default::default()
        }
        .insert(db)
        .await?,
    ];

    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::AssertUnwindSafe(customers.load_one(ledger::Entity, db))
        .catch_unwind()
        .await;
    std::panic::set_hook(hook);

    let payload = outcome
        .err()
        .expect("an aliased TableRef must abort the load");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or_default()
        .to_owned();
    assert!(
        message.starts_with("not implemented: Unsupported TableRef"),
        "{message}"
    );

    Ok(())
}
