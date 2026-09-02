#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, features::*, setup::*};
use pgorm::{
    ActiveValue::{Set, Unchanged},
    DatabaseConnection,
    entity::prelude::*,
    entity::*,
};
use pretty_assertions::assert_eq;
use serde_json::json;

#[pgorm_macros::test]
async fn main() -> Result<(), DbErr> {
    let ctx = TestContext::new("features_schema_string_primary_key_tests").await;
    create_tables(&ctx.db).await?;

    let db = ctx.db.get().await?;
    create_and_update_repository(&db).await?;
    insert_and_delete_repository(&db).await?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:exec.crud.insert+1/test]    zero rows affected on the
// client-supplied-key path fails with RecordNotInserted
pub async fn insert_and_delete_repository(db: &DatabaseConnection) -> Result<(), DbErr> {
    let repository = repository::Model {
        id: "unique-id-001".to_owned(),
        owner: "GC".to_owned(),
        name: "G.C.".to_owned(),
        description: None,
    }
    .into_active_model();

    let result = repository.clone().insert(db).await?;

    assert_eq!(
        result,
        repository::Model {
            id: "unique-id-001".to_owned(),
            owner: "GC".to_owned(),
            name: "G.C.".to_owned(),
            description: None,
        }
    );

    {
        use pgorm::pgorm_query::OnConflict;

        let err = Repository::insert(repository)
            .on_conflict(OnConflict::new().do_nothing().to_owned())
            .exec(db)
            .await;

        assert_eq!(err.err(), Some(DbErr::RecordNotInserted));
    }

    result.delete(db).await?;

    assert_eq!(
        edit_log::Entity::find().all(db).await?,
        [
            edit_log::Model {
                id: 1,
                action: "before_save".into(),
                values: json!({
                    "description": null,
                    "id": "unique-id-001",
                    "name": "G.C.",
                    "owner": "GC",
                }),
            },
            edit_log::Model {
                id: 2,
                action: "after_save".into(),
                values: json!({
                    "description": null,
                    "id": "unique-id-001",
                    "name": "G.C.",
                    "owner": "GC",
                }),
            },
            edit_log::Model {
                id: 3,
                action: "before_delete".into(),
                values: json!({
                    "description": null,
                    "id": "unique-id-001",
                    "name": "G.C.",
                    "owner": "GC",
                }),
            },
            edit_log::Model {
                id: 4,
                action: "after_delete".into(),
                values: json!({
                    "description": null,
                    "id": "unique-id-001",
                    "name": "G.C.",
                    "owner": "GC",
                }),
            },
        ]
    );

    Ok(())
}

// [spec:pgorm:sem:exec.crud.insert+1/test]    the client-supplied primary-key
// path: `last_insert_id` is reconstructed from the cached `ValueTuple`
// [spec:pgorm:sem:query.build.insert/test]    the capture that makes that
// possible: `Insert::add` records the model's primary-key value tuple when the
// entity's key is not auto-increment
// [spec:pgorm:sem:exec.crud.update+3/test]    `UpdateOne::exec` returns the model
// built from the full-column RETURNING, including a column set back to NULL
pub async fn create_and_update_repository(db: &DatabaseConnection) -> Result<(), DbErr> {
    let repository = repository::Model {
        id: "unique-id-002".to_owned(),
        owner: "GC".to_owned(),
        name: "G.C.".to_owned(),
        description: None,
    };

    let res = Repository::insert(repository.clone().into_active_model())
        .exec(db)
        .await?;

    assert_eq!(
        Repository::find().one_opt(db).await?,
        Some(repository.clone())
    );

    assert_eq!(res.last_insert_id, repository.id);

    let updated_active_model = repository::ActiveModel {
        description: Set(Some("description...".to_owned())),
        ..repository.clone().into_active_model()
    };

    let update_res = Repository::update(updated_active_model.clone())?
        .filter(repository::Column::Id.eq("not-exists-id".to_owned()))
        .exec(db)
        .await;

    // [spec:pgorm:sem:exec.crud.update+3] UpdateOne decodes through `one`, so a
    // filter matching zero rows surfaces RecordNotFound.
    assert_eq!(update_res, Err(DbErr::RecordNotFound));

    let update_res = Repository::update(updated_active_model)?
        .filter(repository::Column::Id.eq("unique-id-002".to_owned()))
        .exec(db)
        .await?;

    assert_eq!(
        update_res,
        repository::Model {
            id: "unique-id-002".to_owned(),
            owner: "GC".to_owned(),
            name: "G.C.".to_owned(),
            description: Some("description...".to_owned()),
        }
    );

    let updated_active_model = repository::ActiveModel {
        description: Set(None),
        ..repository.clone().into_active_model()
    };

    let update_res = Repository::update(updated_active_model.clone())?
        .filter(repository::Column::Id.eq("unique-id-002".to_owned()))
        .exec(db)
        .await?;

    assert_eq!(
        update_res,
        repository::Model {
            id: "unique-id-002".to_owned(),
            owner: "GC".to_owned(),
            name: "G.C.".to_owned(),
            description: None,
        }
    );

    Ok(())
}
