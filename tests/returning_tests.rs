#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, bakery_chain::*, setup::*};
use pgorm::{
    ActiveValue::Set, ColumnTrait, DatabaseConnection, IntoActiveModel, ValueHolder,
    entity::prelude::*, types::ToSql,
};
pub use pgorm_query::{Expr, Query, QueryBuilder, Values};
use serde_json::json;

fn holders(values: Values) -> Vec<ValueHolder> {
    values.into_iter().map(ValueHolder).collect()
}

fn params(holders: &[ValueHolder]) -> Vec<&(dyn ToSql + Sync)> {
    holders.iter().map(|v| v as &(dyn ToSql + Sync)).collect()
}

// [spec:pgorm:def:exec.cursor.binding+2/test]    every built statement value is
// wrapped in `ValueHolder` for binding — here `String`, `Double` and `Int`
#[pgorm_macros::test]
async fn main() -> Result<(), DbErr> {
    use bakery::*;

    let ctx = TestContext::new("returning_tests").await;
    create_tables(&ctx.db).await?;
    let db = ctx.db.get().await?;

    let mut insert = Query::insert();
    insert
        .into_table(Entity)
        .columns([Column::Name, Column::ProfitMargin])
        .values_panic(["Bakery Shop".into(), 0.5.into()]);

    let mut update = Query::update();
    update
        .table(Entity)
        .values([
            (Column::Name, "Bakery Shop".into()),
            (Column::ProfitMargin, 0.5.into()),
        ])
        .and_where(Column::Id.eq(1));

    let columns = [Column::Id, Column::Name, Column::ProfitMargin];
    let returning = Query::returning().exprs(columns.into_iter().map(|c| c.into_returning_expr()));

    insert.returning(returning.clone());
    let (sql, values) = insert.build();
    let bound = holders(values);
    let insert_res = db.query_one(&sql, &params(&bound)).await?;
    let _id: i32 = insert_res.try_get("id")?;
    let _name: String = insert_res.try_get("name")?;
    let _profit_margin: f64 = insert_res.try_get("profit_margin")?;

    update.returning(returning.clone());
    let (sql, values) = update.build();
    let bound = holders(values);
    let update_res = db.query_one(&sql, &params(&bound)).await?;
    let _id: i32 = update_res.try_get("id")?;
    let _name: String = update_res.try_get("name")?;
    let _profit_margin: f64 = update_res.try_get("profit_margin")?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:exec.crud.update+3/test]    `UpdateMany::exec_with_returning`
// returns every updated model, and an empty `Vec` on the no-op path
#[pgorm_macros::test]
async fn update_many() {
    pub use common::{TestContext, features::*};
    use edit_log::*;

    let run = || async {
        let ctx = TestContext::new("returning_tests_update_many").await;
        create_tables(&ctx.db).await?;
        let db = ctx.db.get().await?;

        Entity::insert(
            Model {
                id: 1,
                action: "before_save".into(),
                values: json!({ "id": "unique-id-001" }),
            }
            .into_active_model(),
        )
        .exec(&db)
        .await?;

        Entity::insert(
            Model {
                id: 2,
                action: "before_save".into(),
                values: json!({ "id": "unique-id-002" }),
            }
            .into_active_model(),
        )
        .exec(&db)
        .await?;

        Entity::insert(
            Model {
                id: 3,
                action: "before_save".into(),
                values: json!({ "id": "unique-id-003" }),
            }
            .into_active_model(),
        )
        .exec(&db)
        .await?;

        assert_eq!(
            Entity::find().all(&db).await?,
            [
                Model {
                    id: 1,
                    action: "before_save".into(),
                    values: json!({ "id": "unique-id-001" }),
                },
                Model {
                    id: 2,
                    action: "before_save".into(),
                    values: json!({ "id": "unique-id-002" }),
                },
                Model {
                    id: 3,
                    action: "before_save".into(),
                    values: json!({ "id": "unique-id-003" }),
                },
            ]
        );

        // Update many with returning
        assert_eq!(
            Entity::update_many()
                .col_expr(
                    Column::Values,
                    Expr::value(json!({ "remarks": "save log" }))
                )
                .filter(Column::Action.eq("before_save"))
                .exec_with_returning(&db)
                .await?,
            [
                Model {
                    id: 1,
                    action: "before_save".into(),
                    values: json!({ "remarks": "save log" }),
                },
                Model {
                    id: 2,
                    action: "before_save".into(),
                    values: json!({ "remarks": "save log" }),
                },
                Model {
                    id: 3,
                    action: "before_save".into(),
                    values: json!({ "remarks": "save log" }),
                },
            ]
        );

        // No-op
        assert_eq!(
            Entity::update_many()
                .filter(Column::Action.eq("before_save"))
                .exec_with_returning(&db)
                .await?,
            []
        );

        drop(db);
        ctx.delete().await;

        Result::<(), DbErr>::Ok(())
    };

    run().await.unwrap();
}

fn bakery_model(name: &str, margin: f64) -> bakery::ActiveModel {
    bakery::ActiveModel {
        name: Set(name.to_owned()),
        profit_margin: Set(margin),
        ..Default::default()
    }
}

// [spec:pgorm:sem:exec.crud.insert-returning/test]    `exec_with_returning`
// decodes a full-column RETURNING (and fails with RecordNotFound when the
// insert matched nothing); `exec_without_returning` reports rows affected
#[pgorm_macros::test]
async fn insert_returning_modes() -> Result<(), DbErr> {
    use pgorm::PaginatorTrait;
    use pgorm_query::OnConflict;

    let ctx = TestContext::new("returning_tests_insert_modes").await;
    create_tables(&ctx.db).await?;
    let db = ctx.db.get().await?;

    // Every entity column is returned, so the server-assigned primary key is
    // part of the decoded model.
    let inserted = Bakery::insert(bakery_model("SeaSide Bakery", 10.4))
        .exec_with_returning(&db)
        .await?;
    assert_eq!(
        inserted,
        bakery::Model {
            id: 1,
            name: "SeaSide Bakery".to_owned(),
            profit_margin: 10.4,
        }
    );

    // `exec_without_returning` yields only the rows-affected count.
    let affected = Bakery::insert_many([
        bakery_model("Top Bakery", 15.0),
        bakery_model("Third Bakery", 20.5),
        bakery_model("Fourth Bakery", 5.25),
    ])
    .exec_without_returning(&db)
    .await?;
    assert_eq!(affected, 3);
    assert_eq!(Bakery::find().count(&db).await?, 4);
    assert_eq!(
        Bakery::insert(bakery_model("Fifth Bakery", 1.0))
            .exec_without_returning(&db)
            .await?,
        1
    );

    // Nothing to decode: a conflicting insert returns no row, and
    // `SelectorRaw::one_opt` reporting `None` becomes RecordNotFound.
    let conflicted = Bakery::insert(bakery::ActiveModel {
        id: Set(inserted.id),
        name: Set("Duplicate Bakery".to_owned()),
        profit_margin: Set(0.5),
    })
    .on_conflict(OnConflict::do_nothing())
    .exec_with_returning(&db)
    .await;
    assert_eq!(conflicted, Err(DbErr::RecordNotFound));

    // The same conflict is not an error for `exec_without_returning`: no row
    // was written, so the count is zero.
    let skipped = Bakery::insert(bakery::ActiveModel {
        id: Set(inserted.id),
        name: Set("Duplicate Bakery".to_owned()),
        profit_margin: Set(0.5),
    })
    .on_conflict(OnConflict::do_nothing())
    .exec_without_returning(&db)
    .await?;
    assert_eq!(skipped, 0);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:exec.crud.try-insert+1/test]    `TryInsertResult` across all
// three executions: Empty without touching the database, Inserted on success,
// Conflicted from a skipped `ON CONFLICT` insert, and any other error
// propagating
// [spec:pgorm:sem:query.build.insert.empty-failsafe+1/test]    the same three
// entry points reading the one recorded empty state: an insert over an empty
// iterator and an insert of an all-NotSet model both return Empty with the
// database left untouched
#[pgorm_macros::test]
async fn try_insert_result_variants() -> Result<(), DbErr> {
    use pgorm::TryInsertResult;
    use pgorm_query::OnConflict;

    let ctx = TestContext::new("returning_tests_try_insert").await;
    create_tables(&ctx.db).await?;
    let db = ctx.db.get().await?;

    let empty = || std::iter::empty::<bakery::ActiveModel>();

    // A column-less insert statement short-circuits to Empty on each entry
    // point, without issuing SQL.
    assert!(matches!(
        Bakery::insert_many(empty())
            .on_empty_do_nothing()
            .exec(&db)
            .await?,
        TryInsertResult::Empty
    ));
    assert!(matches!(
        Bakery::insert_many(empty())
            .on_empty_do_nothing()
            .exec_without_returning(&db)
            .await?,
        TryInsertResult::Empty
    ));
    assert!(matches!(
        Bakery::insert_many(empty())
            .on_empty_do_nothing()
            .exec_with_returning(&db)
            .await?,
        TryInsertResult::Empty
    ));
    assert_eq!(Bakery::find().all(&db).await?, []);

    // A model that sets no column reaches the same state, so it reads as Empty
    // on all three entry points too rather than inserting a row of defaults.
    let blank = || bakery::ActiveModel {
        ..Default::default()
    };

    assert!(matches!(
        Bakery::insert(blank())
            .on_empty_do_nothing()
            .exec(&db)
            .await?,
        TryInsertResult::Empty
    ));
    assert!(matches!(
        Bakery::insert(blank())
            .on_empty_do_nothing()
            .exec_without_returning(&db)
            .await?,
        TryInsertResult::Empty
    ));
    assert!(matches!(
        Bakery::insert(blank())
            .on_empty_do_nothing()
            .exec_with_returning(&db)
            .await?,
        TryInsertResult::Empty
    ));
    assert_eq!(Bakery::find().all(&db).await?, []);

    // Success wraps the inner result.
    let inserted = Bakery::insert(bakery_model("SeaSide Bakery", 10.4))
        .on_empty_do_nothing()
        .exec_with_returning(&db)
        .await?;
    match inserted {
        TryInsertResult::Inserted(model) => assert_eq!(model.name, "SeaSide Bakery"),
        other => panic!("unexpected result: {other:?}"),
    }

    let counted = Bakery::insert(bakery_model("Top Bakery", 15.0))
        .on_empty_do_nothing()
        .exec_without_returning(&db)
        .await?;
    assert!(matches!(counted, TryInsertResult::Inserted(1)));

    let duplicate = || bakery::ActiveModel {
        id: Set(1),
        name: Set("Duplicate Bakery".to_owned()),
        profit_margin: Set(0.5),
    };
    let on_conflict = OnConflict::do_nothing;

    // An `ON CONFLICT DO NOTHING` clause that skips the row reads as Conflicted
    // on every entry point: `exec` from RecordNotInserted,
    // `exec_without_returning` from a zero rows-affected count, and
    // `exec_with_returning` from the absent RETURNING row.
    assert!(matches!(
        Bakery::insert(duplicate())
            .on_conflict(on_conflict())
            .do_nothing()
            .exec(&db)
            .await?,
        TryInsertResult::Conflicted
    ));
    assert!(matches!(
        Bakery::insert(duplicate())
            .on_conflict(on_conflict())
            .do_nothing()
            .exec_without_returning(&db)
            .await?,
        TryInsertResult::Conflicted
    ));
    assert!(matches!(
        Bakery::insert(duplicate())
            .on_conflict(on_conflict())
            .do_nothing()
            .exec_with_returning(&db)
            .await?,
        TryInsertResult::Conflicted
    ));

    // With no conflict clause to attribute it to, a failing insert propagates
    // its error rather than being folded into a variant.
    assert!(matches!(
        Bakery::insert(duplicate())
            .on_empty_do_nothing()
            .exec_with_returning(&db)
            .await,
        Err(DbErr::Postgres(_))
    ));

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:exec.crud.update+3/test]    the no-op short-circuit of
// `Updater::exec` and `UpdateOne::exec`, plus `check_record_exists`
#[pgorm_macros::test]
async fn update_noop_and_record_check() -> Result<(), DbErr> {
    use pgorm::{ActiveValue::Unchanged, ColumnTrait, Updater};

    let ctx = TestContext::new("returning_tests_update_noop").await;
    create_tables(&ctx.db).await?;
    let db = ctx.db.get().await?;

    let seaside = Bakery::insert(bakery_model("SeaSide Bakery", 10.4))
        .exec_with_returning(&db)
        .await?;

    // `Updater::exec` short-circuits when there is nothing to SET: the filter
    // matches an existing row, yet no rows are reported affected and the row
    // is untouched.
    let noop = Bakery::update_many()
        .filter(bakery::Column::Id.eq(seaside.id))
        .exec(&db)
        .await?;
    assert_eq!(noop, pgorm::UpdateResult { rows_affected: 0 });
    assert_eq!(Bakery::find_by_id(seaside.id).one(&db).await?, seaside);

    // With something to SET the same call reports the rows it changed.
    let applied = Bakery::update_many()
        .col_expr(bakery::Column::ProfitMargin, Expr::value(12.5_f64))
        .filter(bakery::Column::Id.eq(seaside.id))
        .exec(&db)
        .await?;
    assert_eq!(applied, pgorm::UpdateResult { rows_affected: 1 });

    // `check_record_exists` turns "zero rows affected" into RecordNotUpdated.
    let mut update = Query::update();
    update
        .table(bakery::Entity)
        .values([(bakery::Column::Name, "Nowhere Bakery".into())])
        .and_where(bakery::Column::Id.eq(9999));

    assert_eq!(
        Updater::new(update.clone()).exec(&db).await?,
        pgorm::UpdateResult { rows_affected: 0 }
    );
    assert_eq!(
        Updater::new(update).check_record_exists().exec(&db).await,
        Err(DbErr::RecordNotUpdated)
    );

    // On `UpdateOne`'s no-op path nothing is written; the current model is
    // re-fetched by primary key instead.
    let refetched = Bakery::update(bakery::ActiveModel {
        id: Unchanged(seaside.id),
        name: Unchanged(seaside.name.clone()),
        profit_margin: Unchanged(seaside.profit_margin),
    })?
    .exec(&db)
    .await?;
    assert_eq!(
        refetched,
        bakery::Model {
            id: seaside.id,
            name: seaside.name.clone(),
            profit_margin: 12.5,
        }
    );

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:def:exec.crud.exec-result/test]    `ExecResult` is a transparent
// wrapper over the rows-affected `u64` and exposes nothing else
#[pgorm_macros::test]
async fn exec_result_is_a_transparent_row_count() {
    use pgorm::ExecResult;

    assert_eq!(
        std::mem::size_of::<ExecResult>(),
        std::mem::size_of::<u64>()
    );
    assert_eq!(
        std::mem::align_of::<ExecResult>(),
        std::mem::align_of::<u64>()
    );

    // The only accessor is `rows_affected`; there is no `last_insert_id`.
    let accessor: fn(&ExecResult) -> u64 = ExecResult::rows_affected;
    let _ = accessor;
}
