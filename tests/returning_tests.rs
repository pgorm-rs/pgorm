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
    let (sql, values) = insert.build(QueryBuilder);
    let bound = holders(values);
    let insert_res = db.query_one(&sql, &params(&bound)).await?;
    let _id: i32 = insert_res.try_get("id")?;
    let _name: String = insert_res.try_get("name")?;
    let _profit_margin: f64 = insert_res.try_get("profit_margin")?;

    update.returning(returning.clone());
    let (sql, values) = update.build(QueryBuilder);
    let bound = holders(values);
    let update_res = db.query_one(&sql, &params(&bound)).await?;
    let _id: i32 = update_res.try_get("id")?;
    let _name: String = update_res.try_get("name")?;
    let _profit_margin: f64 = update_res.try_get("profit_margin")?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

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
