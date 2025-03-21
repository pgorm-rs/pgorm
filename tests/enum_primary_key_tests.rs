#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, features::*, setup::*};
use pgorm::{
    ActiveEnum as ActiveEnumTrait, DatabasePool,
    entity::prelude::*,
    entity::*,
    pgorm_query::{BinOper, Expr},
};
use pretty_assertions::assert_eq;

#[pgorm_macros::test]
async fn main() -> Result<(), DbErr> {
    let ctx = TestContext::new("enum_primary_key_tests").await;
    create_tables(&ctx.db).await?;
    insert_teas(&ctx.db).await?;
    ctx.delete().await;

    Ok(())
}

pub async fn insert_teas(db: &DatabasePool) -> Result<(), DbErr> {
    use teas::*;

    let model = Model {
        id: Tea::EverydayTea,
        category: None,
        color: None,
    };

    assert_eq!(
        model,
        ActiveModel {
            id: Set(Tea::EverydayTea),
            category: Set(None),
            color: Set(None),
        }
        .insert(db)
        .await?
    );
    assert_eq!(model, Entity::find().one(db).await?.unwrap());
    assert_eq!(
        model,
        Entity::find()
            .filter(Column::Id.is_not_null())
            .filter(Column::Category.is_null())
            .filter(Column::Color.is_null())
            .one(db)
            .await?
            .unwrap()
    );

    // UNIQUE constraint failed
    assert!(
        ActiveModel {
            id: Set(Tea::EverydayTea),
            category: Set(Some(Category::Big)),
            color: Set(Some(Color::Black)),
        }
        .insert(db)
        .await
        .is_err()
    );

    // UNIQUE constraint failed
    assert!(
        Entity::insert(ActiveModel {
            id: Set(Tea::EverydayTea),
            category: Set(Some(Category::Big)),
            color: Set(Some(Color::Black)),
        })
        .exec(db)
        .await
        .is_err()
    );

    let _ = ActiveModel {
        category: Set(Some(Category::Big)),
        color: Set(Some(Color::Black)),
        ..model.into_active_model()
    }
    .save(db)
    .await?;

    let model = Entity::find().one(db).await?.unwrap();
    assert_eq!(
        model,
        Model {
            id: Tea::EverydayTea,
            category: Some(Category::Big),
            color: Some(Color::Black),
        }
    );
    assert_eq!(
        model,
        Entity::find()
            .filter(Column::Id.eq(Tea::EverydayTea))
            .filter(Column::Category.eq(Category::Big))
            .filter(Column::Color.eq(Color::Black))
            .one(db)
            .await?
            .unwrap()
    );
    assert_eq!(
        model,
        Entity::find()
            .filter(
                Expr::col(Column::Id)
                    .binary(BinOper::In, Expr::tuple([Tea::EverydayTea.as_enum()]))
            )
            .one(db)
            .await?
            .unwrap()
    );
    // Equivalent to the above.
    assert_eq!(
        model,
        Entity::find()
            .filter(Column::Id.is_in([Tea::EverydayTea]))
            .one(db)
            .await?
            .unwrap()
    );

    let res = model.delete(db).await?;

    assert_eq!(res.rows_affected, 1);
    assert_eq!(Entity::find().one(db).await?, None);

    Ok(())
}
