#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, features::*, setup::*};
use pgorm::{
    ActiveValue::Set, DatabaseConnection, Delete, IntoActiveModel, Iterable, QueryTrait, Update,
    entity::prelude::*,
};
use pgorm_query::{Expr, Query};
use pretty_assertions::assert_eq;

#[pgorm_macros::test]
async fn main() -> Result<(), DbErr> {
    let ctx = TestContext::new("dyn_table_name_tests").await;
    create_tables(&ctx.db).await?;
    let db = ctx.db.get().await?;

    dyn_table_name_lazy_static(&db).await?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:exec.crud.update+1/test]    `UpdateMany::exec` returns
// `UpdateResult { rows_affected }`
// [spec:pgorm:sem:exec.crud.delete/test]    `DeleteMany::exec` returns
// `DeleteResult { rows_affected }`
pub async fn dyn_table_name_lazy_static(db: &DatabaseConnection) -> Result<(), DbErr> {
    use dyn_table_name_lazy_static::*;

    for i in 1..=2 {
        let entity = Entity {
            table_name: TableName::from_str_truncate(format!("dyn_table_name_lazy_static_{}", i)),
        };

        let model = Model {
            id: 1,
            name: "1st Row".into(),
        };
        // Prepare insert statement
        let mut insert = Entity::insert(model.clone().into_active_model());
        // Reset the table name of insert statement
        insert.query().into_table(entity.table_ref());
        // Execute the insert statement
        assert_eq!(insert.exec(db).await?.last_insert_id, 1);

        // Prepare select statement
        let mut select = Entity::find();
        // Override the select statement
        *QueryTrait::query(&mut select) = Query::select()
            .exprs(Column::iter().map(|col| col.select_as(Expr::col(col))))
            .from(entity.table_ref())
            .to_owned();
        // Execute the select statement
        assert_eq!(select.clone().one(db).await?, model.clone());

        // Prepare update statement
        let update = Update::many(entity).set(ActiveModel {
            name: Set("1st Row (edited)".into()),
            ..model.clone().into_active_model()
        });
        // Execute the update statement
        assert_eq!(update.exec(db).await?.rows_affected, 1);

        assert_eq!(
            select.clone().one(db).await?,
            Model {
                id: 1,
                name: "1st Row (edited)".into(),
            }
        );

        // Prepare delete statement
        let delete = Delete::many(entity).filter(Expr::col(Column::Id).eq(1));
        // Execute the delete statement
        assert_eq!(delete.exec(db).await?.rows_affected, 1);
        assert_eq!(select.one_opt(db).await?, None);
    }

    Ok(())
}
