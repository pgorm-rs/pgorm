#![allow(unused_imports, dead_code)]

pub mod common;

use active_enum::Entity as ActiveEnumEntity;
pub use common::{TestContext, features::*, setup::*};
use pgorm::{
    ActiveEnum as ActiveEnumTrait,
    ActiveValue::{Set, Unchanged},
    DatabaseConnection, QueryTrait,
    entity::prelude::*,
    entity::*,
    pgorm_query::{BinOper, Expr, QueryBuilder, QueryStatementWriter},
};
use pretty_assertions::assert_eq;

#[pgorm_macros::test]
async fn main() -> Result<(), DbErr> {
    let ctx = TestContext::new("active_enum_tests").await;
    create_tables(&ctx.db).await?;

    let db = ctx.db.get().await?;
    insert_active_enum(&db).await?;
    insert_active_enum_child(&db).await?;
    insert_active_enum_vec(&db).await?;
    find_related_active_enum(&db).await?;
    find_linked_active_enum(&db).await?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

pub async fn insert_active_enum(db: &DatabaseConnection) -> Result<(), DbErr> {
    use active_enum::*;

    let model = Model {
        id: 1,
        category: None,
        color: None,
        tea: None,
    };

    assert_eq!(
        model,
        ActiveModel {
            category: Set(None),
            color: Set(None),
            tea: Set(None),
            ..Default::default()
        }
        .insert(db)
        .await?
    );
    assert_eq!(model, Entity::find().one(db).await?);
    assert_eq!(
        model,
        Entity::find()
            .filter(Column::Id.is_not_null())
            .filter(Column::Category.is_null())
            .filter(Column::Color.is_null())
            .filter(Column::Tea.is_null())
            .one(db)
            .await?
    );

    let _ = ActiveModel {
        category: Set(Some(Category::Big)),
        color: Set(Some(Color::Black)),
        tea: Set(Some(Tea::EverydayTea)),
        ..model.into_active_model()
    }
    .save(db)
    .await?;

    let model = Entity::find().one(db).await?;
    assert_eq!(
        model,
        Model {
            id: 1,
            category: Some(Category::Big),
            color: Some(Color::Black),
            tea: Some(Tea::EverydayTea),
        }
    );
    assert_eq!(
        model,
        Entity::find()
            .filter(Column::Id.eq(1))
            .filter(Column::Category.eq(Category::Big))
            .filter(Column::Color.eq(Color::Black))
            .filter(Column::Tea.eq(Tea::EverydayTea))
            .one(db)
            .await?
    );

    assert_eq!(
        model,
        Entity::find()
            .filter(
                Expr::col(Column::Tea)
                    .binary(BinOper::In, Expr::tuple([Tea::EverydayTea.as_enum()]))
            )
            .one(db)
            .await?
    );
    // Equivalent to the above.
    let select_with_tea_in =
        Entity::find().filter(Column::Tea.is_in([Tea::EverydayTea, Tea::BreakfastTea]));
    assert_eq!(
        select_with_tea_in.as_query().to_string(QueryBuilder),
        [
            r#"SELECT "active_enum"."id","#,
            r#""active_enum"."category","#,
            r#""active_enum"."color","#,
            r#"CAST("active_enum"."tea" AS text)"#,
            r#"FROM "active_enum""#,
            r#"WHERE "active_enum"."tea" IN (CAST('EverydayTea' AS tea), CAST('BreakfastTea' AS tea))"#,
        ]
        .join(" ")
    );
    assert_eq!(model, select_with_tea_in.one(db).await?);

    assert_eq!(
        model,
        Entity::find()
            .filter(Column::Tea.is_not_null())
            .filter(
                Expr::col(Column::Tea)
                    .binary(BinOper::NotIn, Expr::tuple([Tea::BreakfastTea.as_enum()]))
            )
            .one(db)
            .await?
    );
    // Equivalent to the above.
    let select_with_tea_not_in = Entity::find()
        .filter(Column::Tea.is_not_null())
        .filter(Column::Tea.is_not_in([Tea::BreakfastTea]));

    assert_eq!(
        select_with_tea_not_in.as_query().to_string(QueryBuilder),
        [
            r#"SELECT "active_enum"."id","#,
            r#""active_enum"."category","#,
            r#""active_enum"."color","#,
            r#"CAST("active_enum"."tea" AS text)"#,
            r#"FROM "active_enum""#,
            r#"WHERE "active_enum"."tea" IS NOT NULL"#,
            r#"AND "active_enum"."tea" NOT IN (CAST('BreakfastTea' AS tea))"#,
        ]
        .join(" ")
    );

    assert_eq!(model, select_with_tea_not_in.one(db).await?);

    // String enums are compared alphabetically.
    // 'B' < 'S', so Big is considered "smaller" than Small.
    assert_eq!(
        model,
        Entity::find()
            .filter(Column::Category.lt(Category::Small))
            .one(db)
            .await?
    );

    // Integer enums are compared by value.
    // 0 <= 1, so Black is considered "smaller or equal to" White.
    assert_eq!(
        model,
        Entity::find()
            .filter(Column::Color.lte(Color::White))
            .one(db)
            .await?
    );

    // Postgres enums are compared by their definition order
    // (see https://www.postgresql.org/docs/current/datatype-enum.html#DATATYPE-ENUM-ORDERING).
    // Tea was defined as ('EverydayTea', 'BreakfastTea'), so EverydayTea is considered "smaller" than BreakfastTea.
    assert_eq!(
        model,
        Entity::find()
            .filter(Column::Tea.lt(Tea::BreakfastTea))
            .one(db)
            .await?
    );

    let res = model.delete(db).await?;

    assert_eq!(res.rows_affected, 1);
    assert_eq!(Entity::find().one_opt(db).await?, None);

    Ok(())
}

pub async fn insert_active_enum_child(db: &DatabaseConnection) -> Result<(), DbErr> {
    use active_enum_child::*;

    active_enum::ActiveModel {
        category: Set(Some(Category::Small)),
        color: Set(Some(Color::White)),
        tea: Set(Some(Tea::BreakfastTea)),
        ..Default::default()
    }
    .insert(db)
    .await?;

    let am = ActiveModel {
        parent_id: Set(2),
        category: Set(None),
        color: Set(None),
        tea: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await?;

    let model = Entity::find().one(db).await?;
    assert_eq!(
        model,
        Model {
            id: 1,
            parent_id: 2,
            category: None,
            color: None,
            tea: None,
        }
    );
    assert_eq!(
        model,
        Entity::find()
            .filter(Column::Id.is_not_null())
            .filter(Column::Category.is_null())
            .filter(Column::Color.is_null())
            .filter(Column::Tea.is_null())
            .one(db)
            .await?
    );

    ActiveModel {
        category: Set(Some(Category::Big)),
        color: Set(Some(Color::Black)),
        tea: Set(Some(Tea::EverydayTea)),
        ..am.into_active_model()
    }
    .save(db)
    .await?;

    let model = Entity::find().one(db).await?;
    assert_eq!(
        model,
        Model {
            id: 1,
            parent_id: 2,
            category: Some(Category::Big),
            color: Some(Color::Black),
            tea: Some(Tea::EverydayTea),
        }
    );
    assert_eq!(
        model,
        Entity::find()
            .filter(Column::Id.eq(1))
            .filter(Column::Category.eq(Category::Big))
            .filter(Column::Color.eq(Color::Black))
            .filter(Column::Tea.eq(Tea::EverydayTea))
            .one(db)
            .await?
    );

    Ok(())
}

pub async fn insert_active_enum_vec(db: &DatabaseConnection) -> Result<(), DbErr> {
    use categories::*;

    let model = Model {
        id: 1,
        categories: None,
    };

    assert_eq!(
        model,
        ActiveModel {
            id: Set(1),
            categories: Set(None),
            ..Default::default()
        }
        .insert(db)
        .await?
    );
    assert_eq!(model, Entity::find().one(db).await?);
    assert_eq!(
        model,
        Entity::find()
            .filter(Column::Id.is_not_null())
            .filter(Column::Categories.is_null())
            .one(db)
            .await?
    );

    let _ = ActiveModel {
        id: Set(1),
        categories: Set(Some(vec![Category::Big, Category::Small])),
        ..model.into_active_model()
    }
    .save(db)
    .await?;

    let model = Entity::find().one(db).await?;
    assert_eq!(
        model,
        Model {
            id: 1,
            categories: Some(vec![Category::Big, Category::Small]),
        }
    );
    assert_eq!(
        model,
        Entity::find()
            .filter(Column::Id.eq(1))
            .filter(Expr::cust_with_values(
                r#"$1 = ANY("categories")"#,
                vec![Category::Big]
            ))
            .one(db)
            .await?
    );

    let res = model.delete(db).await?;

    assert_eq!(res.rows_affected, 1);
    assert_eq!(Entity::find().one_opt(db).await?, None);

    Ok(())
}

pub async fn find_related_active_enum(db: &DatabaseConnection) -> Result<(), DbErr> {
    assert_eq!(
        active_enum::Model {
            id: 2,
            category: None,
            color: None,
            tea: None,
        }
        .find_related(ActiveEnumChild)
        .all(db)
        .await?,
        [active_enum_child::Model {
            id: 1,
            parent_id: 2,
            category: Some(Category::Big),
            color: Some(Color::Black),
            tea: Some(Tea::EverydayTea),
        }]
    );
    assert_eq!(
        ActiveEnumEntity::find()
            .find_with_related(ActiveEnumChild)
            .all(db)
            .await?,
        [(
            active_enum::Model {
                id: 2,
                category: Some(Category::Small),
                color: Some(Color::White),
                tea: Some(Tea::BreakfastTea),
            },
            vec![active_enum_child::Model {
                id: 1,
                parent_id: 2,
                category: Some(Category::Big),
                color: Some(Color::Black),
                tea: Some(Tea::EverydayTea),
            }]
        )]
    );
    assert_eq!(
        ActiveEnumEntity::find()
            .find_also_related(ActiveEnumChild)
            .all(db)
            .await?,
        [(
            active_enum::Model {
                id: 2,
                category: Some(Category::Small),
                color: Some(Color::White),
                tea: Some(Tea::BreakfastTea),
            },
            Some(active_enum_child::Model {
                id: 1,
                parent_id: 2,
                category: Some(Category::Big),
                color: Some(Color::Black),
                tea: Some(Tea::EverydayTea),
            })
        )]
    );

    assert_eq!(
        active_enum_child::Model {
            id: 1,
            parent_id: 2,
            category: None,
            color: None,
            tea: None,
        }
        .find_related(ActiveEnum)
        .all(db)
        .await?,
        [active_enum::Model {
            id: 2,
            category: Some(Category::Small),
            color: Some(Color::White),
            tea: Some(Tea::BreakfastTea),
        }]
    );
    assert_eq!(
        ActiveEnumChild::find()
            .find_with_related(ActiveEnum)
            .all(db)
            .await?,
        [(
            active_enum_child::Model {
                id: 1,
                parent_id: 2,
                category: Some(Category::Big),
                color: Some(Color::Black),
                tea: Some(Tea::EverydayTea),
            },
            vec![active_enum::Model {
                id: 2,
                category: Some(Category::Small),
                color: Some(Color::White),
                tea: Some(Tea::BreakfastTea),
            }]
        )]
    );
    assert_eq!(
        ActiveEnumChild::find()
            .find_also_related(ActiveEnum)
            .all(db)
            .await?,
        [(
            active_enum_child::Model {
                id: 1,
                parent_id: 2,
                category: Some(Category::Big),
                color: Some(Color::Black),
                tea: Some(Tea::EverydayTea),
            },
            Some(active_enum::Model {
                id: 2,
                category: Some(Category::Small),
                color: Some(Color::White),
                tea: Some(Tea::BreakfastTea),
            })
        )]
    );

    Ok(())
}

pub async fn find_linked_active_enum(db: &DatabaseConnection) -> Result<(), DbErr> {
    assert_eq!(
        active_enum::Model {
            id: 2,
            category: None,
            color: None,
            tea: None,
        }
        .find_linked(active_enum::ActiveEnumChildLink)
        .all(db)
        .await?,
        [active_enum_child::Model {
            id: 1,
            parent_id: 2,
            category: Some(Category::Big),
            color: Some(Color::Black),
            tea: Some(Tea::EverydayTea),
        }]
    );
    assert_eq!(
        ActiveEnumEntity::find()
            .find_also_linked(active_enum::ActiveEnumChildLink)
            .all(db)
            .await?,
        [(
            active_enum::Model {
                id: 2,
                category: Some(Category::Small),
                color: Some(Color::White),
                tea: Some(Tea::BreakfastTea),
            },
            Some(active_enum_child::Model {
                id: 1,
                parent_id: 2,
                category: Some(Category::Big),
                color: Some(Color::Black),
                tea: Some(Tea::EverydayTea),
            })
        )]
    );
    assert_eq!(
        ActiveEnumEntity::find()
            .find_with_linked(active_enum::ActiveEnumChildLink)
            .all(db)
            .await?,
        [(
            active_enum::Model {
                id: 2,
                category: Some(Category::Small),
                color: Some(Color::White),
                tea: Some(Tea::BreakfastTea),
            },
            vec![active_enum_child::Model {
                id: 1,
                parent_id: 2,
                category: Some(Category::Big),
                color: Some(Color::Black),
                tea: Some(Tea::EverydayTea),
            }]
        )]
    );

    assert_eq!(
        active_enum_child::Model {
            id: 1,
            parent_id: 2,
            category: None,
            color: None,
            tea: None,
        }
        .find_linked(active_enum_child::ActiveEnumLink)
        .all(db)
        .await?,
        [active_enum::Model {
            id: 2,
            category: Some(Category::Small),
            color: Some(Color::White),
            tea: Some(Tea::BreakfastTea),
        }]
    );
    assert_eq!(
        ActiveEnumChild::find()
            .find_also_linked(active_enum_child::ActiveEnumLink)
            .all(db)
            .await?,
        [(
            active_enum_child::Model {
                id: 1,
                parent_id: 2,
                category: Some(Category::Big),
                color: Some(Color::Black),
                tea: Some(Tea::EverydayTea),
            },
            Some(active_enum::Model {
                id: 2,
                category: Some(Category::Small),
                color: Some(Color::White),
                tea: Some(Tea::BreakfastTea),
            })
        )]
    );
    assert_eq!(
        ActiveEnumChild::find()
            .find_with_linked(active_enum_child::ActiveEnumLink)
            .all(db)
            .await?,
        [(
            active_enum_child::Model {
                id: 1,
                parent_id: 2,
                category: Some(Category::Big),
                color: Some(Color::Black),
                tea: Some(Tea::EverydayTea),
            },
            vec![active_enum::Model {
                id: 2,
                category: Some(Category::Small),
                color: Some(Color::White),
                tea: Some(Tea::BreakfastTea),
            }]
        )]
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    pub use pgorm::QueryTrait;
    pub use pgorm::pgorm_query::{QueryBuilder, QueryStatementWriter};
    pub use pretty_assertions::assert_eq;

    #[test]
    fn active_enum_find_related() {
        let active_enum_model = active_enum::Model {
            id: 1,
            category: None,
            color: None,
            tea: None,
        };
        let select = active_enum_model.find_related(ActiveEnumChild);
        assert_eq!(
            select.as_query().to_string(QueryBuilder),
            [
                r#"SELECT "active_enum_child"."id", "active_enum_child"."parent_id", "active_enum_child"."category", "active_enum_child"."color", CAST("active_enum_child"."tea" AS text)"#,
                r#"FROM "active_enum_child""#,
                r#"INNER JOIN "active_enum" ON "active_enum"."id" = "active_enum_child"."parent_id""#,
                r#"WHERE "active_enum"."id" = 1"#,
            ]
            .join(" ")
        );

        let select = ActiveEnumEntity::find().find_also_related(ActiveEnumChild);
        assert_eq!(
            select.as_query().to_string(QueryBuilder),
            [
                r#"SELECT "active_enum"."id" AS "A_id", "active_enum"."category" AS "A_category", "active_enum"."color" AS "A_color", CAST("active_enum"."tea" AS text) AS "A_tea","#,
                r#""active_enum_child"."id" AS "B_id", "active_enum_child"."parent_id" AS "B_parent_id", "active_enum_child"."category" AS "B_category", "active_enum_child"."color" AS "B_color", CAST("active_enum_child"."tea" AS text) AS "B_tea""#,
                r#"FROM "active_enum""#,
                r#"LEFT JOIN "active_enum_child" ON "active_enum"."id" = "active_enum_child"."parent_id""#,
            ]
            .join(" ")
        );
    }

    #[test]
    fn active_enum_find_linked() {
        let active_enum_model = active_enum::Model {
            id: 1,
            category: None,
            color: None,
            tea: None,
        };
        let select = active_enum_model.find_linked(active_enum::ActiveEnumChildLink);
        assert_eq!(
            select.as_query().to_string(QueryBuilder),
            [
                r#"SELECT "active_enum_child"."id", "active_enum_child"."parent_id", "active_enum_child"."category", "active_enum_child"."color", CAST("active_enum_child"."tea" AS text)"#,
                r#"FROM "active_enum_child""#,
                r#"INNER JOIN "active_enum" AS "r0" ON "r0"."id" = "active_enum_child"."parent_id""#,
                r#"WHERE "r0"."id" = 1"#,
            ]
            .join(" ")
        );

        let select = ActiveEnumEntity::find().find_also_linked(active_enum::ActiveEnumChildLink);
        assert_eq!(
            select.as_query().to_string(QueryBuilder),
            [
                r#"SELECT "active_enum"."id" AS "A_id", "active_enum"."category" AS "A_category", "active_enum"."color" AS "A_color", CAST("active_enum"."tea" AS text) AS "A_tea","#,
                r#""r0"."id" AS "B_id", "r0"."parent_id" AS "B_parent_id", "r0"."category" AS "B_category", "r0"."color" AS "B_color", CAST("r0"."tea" AS text) AS "B_tea""#,
                r#"FROM "active_enum""#,
                r#"LEFT JOIN "active_enum_child" AS "r0" ON "active_enum"."id" = "r0"."parent_id""#,
            ]
            .join(" ")
        );
    }

    #[test]
    fn active_enum_child_find_related() {
        let active_enum_child_model = active_enum_child::Model {
            id: 1,
            parent_id: 2,
            category: None,
            color: None,
            tea: None,
        };
        let select = active_enum_child_model.find_related(ActiveEnum);
        assert_eq!(
            select.as_query().to_string(QueryBuilder),
            [
                r#"SELECT "active_enum"."id", "active_enum"."category", "active_enum"."color", CAST("active_enum"."tea" AS text)"#,
                r#"FROM "active_enum""#,
                r#"INNER JOIN "active_enum_child" ON "active_enum_child"."parent_id" = "active_enum"."id""#,
                r#"WHERE "active_enum_child"."id" = 1"#,
            ]
            .join(" ")
        );

        let select = ActiveEnumChild::find().find_also_related(ActiveEnum);
        assert_eq!(
            select.as_query().to_string(QueryBuilder),
            [
                r#"SELECT "active_enum_child"."id" AS "A_id", "active_enum_child"."parent_id" AS "A_parent_id", "active_enum_child"."category" AS "A_category", "active_enum_child"."color" AS "A_color", CAST("active_enum_child"."tea" AS text) AS "A_tea","#,
                r#""active_enum"."id" AS "B_id", "active_enum"."category" AS "B_category", "active_enum"."color" AS "B_color", CAST("active_enum"."tea" AS text) AS "B_tea""#,
                r#"FROM "active_enum_child""#,
                r#"LEFT JOIN "active_enum" ON "active_enum_child"."parent_id" = "active_enum"."id""#,
            ]
            .join(" ")
        );
    }

    #[test]
    fn active_enum_child_find_linked() {
        let active_enum_child_model = active_enum_child::Model {
            id: 1,
            parent_id: 2,
            category: None,
            color: None,
            tea: None,
        };
        let select = active_enum_child_model.find_linked(active_enum_child::ActiveEnumLink);
        assert_eq!(
            select.as_query().to_string(QueryBuilder),
            [
                r#"SELECT "active_enum"."id", "active_enum"."category", "active_enum"."color", CAST("active_enum"."tea" AS text)"#,
                r#"FROM "active_enum""#,
                r#"INNER JOIN "active_enum_child" AS "r0" ON "r0"."parent_id" = "active_enum"."id""#,
                r#"WHERE "r0"."id" = 1"#,
            ]
            .join(" ")
        );

        let select = ActiveEnumChild::find().find_also_linked(active_enum_child::ActiveEnumLink);
        assert_eq!(
            select.as_query().to_string(QueryBuilder),
            [
                r#"SELECT "active_enum_child"."id" AS "A_id", "active_enum_child"."parent_id" AS "A_parent_id", "active_enum_child"."category" AS "A_category", "active_enum_child"."color" AS "A_color", CAST("active_enum_child"."tea" AS text) AS "A_tea","#,
                r#""r0"."id" AS "B_id", "r0"."category" AS "B_category", "r0"."color" AS "B_color", CAST("r0"."tea" AS text) AS "B_tea""#,
                r#"FROM "active_enum_child""#,
                r#"LEFT JOIN "active_enum" AS "r0" ON "active_enum_child"."parent_id" = "r0"."id""#,
            ]
            .join(" ")
        );
    }

    #[test]
    fn create_enum_from() {
        use pgorm::Schema;

        let schema = Schema::new();

        assert_eq!(
            schema
                .create_enum_from_entity(active_enum::Entity)
                .iter()
                .map(|stmt| stmt.to_string(QueryBuilder))
                .collect::<Vec<_>>(),
            [r#"CREATE TYPE "tea" AS ENUM ('EverydayTea', 'BreakfastTea')"#.to_owned()]
        );

        assert_eq!(
            schema
                .create_enum_from_active_enum::<Tea>()
                .to_string(QueryBuilder),
            r#"CREATE TYPE "tea" AS ENUM ('EverydayTea', 'BreakfastTea')"#
        );
    }

    #[test]
    fn display_test() {
        assert_eq!(format!("{}", Tea::BreakfastTea), "BreakfastTea");
        assert_eq!(format!("{}", DisplayTea::BreakfastTea), "Breakfast");
        assert_eq!(format!("{}", Tea::EverydayTea), "EverydayTea");
        assert_eq!(format!("{}", DisplayTea::EverydayTea), "Everyday");
    }
}
