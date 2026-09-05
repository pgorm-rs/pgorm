#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, features::*, setup::*};
use pgorm::entity::prelude::*;
use pretty_assertions::assert_eq;

#[pgorm_macros::test]
async fn main() -> Result<(), Error> {
    let ctx = TestContext::new("self_join_tests").await;
    create_tables(&ctx.db).await?;

    let db = ctx.db.get().await?;
    create_metadata(&db).await?;
    drop(db);

    ctx.delete().await;

    Ok(())
}

pub async fn create_metadata(db: &DatabaseConnection) -> Result<(), Error> {
    let model = self_join::Model {
        uuid: Uuid::new_v4(),
        uuid_ref: None,
        time: Some(Time::from_hms_opt(1, 00, 00).unwrap()),
    };

    model.clone().into_active_model().insert(db).await?;

    let linked_model = self_join::Model {
        uuid: Uuid::new_v4(),
        uuid_ref: Some(model.clone().uuid),
        time: Some(Time::from_hms_opt(2, 00, 00).unwrap()),
    };

    linked_model.clone().into_active_model().insert(db).await?;

    let not_linked_model = self_join::Model {
        uuid: Uuid::new_v4(),
        uuid_ref: None,
        time: Some(Time::from_hms_opt(3, 00, 00).unwrap()),
    };

    not_linked_model
        .clone()
        .into_active_model()
        .insert(db)
        .await?;

    assert_eq!(
        model
            .find_linked(RelatedLink::to(self_join::Entity))
            .all(db)
            .await?,
        []
    );

    assert_eq!(
        linked_model
            .find_linked(RelatedLink::to(self_join::Entity))
            .all(db)
            .await?,
        std::slice::from_ref(&model)
    );

    assert_eq!(
        not_linked_model
            .find_linked(RelatedLink::to(self_join::Entity))
            .all(db)
            .await?,
        []
    );

    assert_eq!(
        self_join::Entity::find()
            .find_also_linked(RelatedLink::to(self_join::Entity))
            .order_by_asc(self_join::Column::Time)
            .all(db)
            .await?,
        [
            (model.clone(), None),
            (linked_model, Some(model)),
            (not_linked_model, None),
        ]
    );

    Ok(())
}

// [spec:pgorm:req:entity.relation.linked+2/test]    a self-relation is the case
// `RelatedLink` exists for: the link form aliases the joined table, so the
// entity's own table can be joined a second time without being named twice
#[test]
fn self_related_link_aliases_the_second_copy() {
    use pgorm::QueryTrait;

    assert_eq!(
        self_join::Entity::find()
            .find_also_linked(RelatedLink::to(self_join::Entity))
            .as_query()
            .to_string(),
        [
            r#"SELECT "self_join"."uuid" AS "A_uuid", "self_join"."uuid_ref" AS "A_uuid_ref","#,
            r#""self_join"."time" AS "A_time", "r0"."uuid" AS "B_uuid","#,
            r#""r0"."uuid_ref" AS "B_uuid_ref", "r0"."time" AS "B_time""#,
            r#"FROM "self_join""#,
            r#"LEFT JOIN "self_join" AS "r0" ON "self_join"."uuid_ref" = "r0"."uuid""#,
        ]
        .join(" ")
    );
}
