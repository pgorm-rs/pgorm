#![allow(unused_imports, dead_code)]

pub mod common;
pub use common::{TestContext, features::*, setup::*};
use pgorm::entity::prelude::*;
use pretty_assertions::assert_eq;

#[pgorm_macros::test]
async fn main() -> Result<(), Error> {
    let ctx = TestContext::new("bakery_chain_schema_timestamp_tests").await;
    create_tables(&ctx.db).await?;

    let db = ctx.db.get().await?;
    create_applog(&db).await?;
    create_satellites_log(&db).await?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

pub async fn create_applog(db: &DatabaseConnection) -> Result<(), Error> {
    let log = applog::Model {
        id: 1,
        action: "Testing".to_owned(),
        json: Json::String("HI".to_owned()),
        created_at: "2021-09-17T17:50:20+08:00".parse().unwrap(),
    };

    let res = Applog::insert(log.clone().into_active_model())
        .exec(db)
        .await?;

    assert_eq!(log.id, res.last_insert_id);

    let found = Applog::find().one(db).await?;
    assert_eq!(found, log);
    assert_eq!(found.created_at.to_rfc3339(), "2021-09-17T09:50:20+00:00");

    Ok(())
}

pub async fn create_satellites_log(db: &DatabaseConnection) -> Result<(), Error> {
    let archive = satellite::Model {
        id: 1,
        satellite_name: "Sea-00001-2022".to_owned(),
        launch_date: "2022-01-07T12:11:23Z".parse().unwrap(),
        deployment_date: "2022-01-07T12:11:23Z".parse().unwrap(),
    };

    let res = Satellite::insert(archive.clone().into_active_model())
        .exec(db)
        .await?;

    assert_eq!(archive.id, res.last_insert_id);

    let found = Satellite::find().one(db).await?;
    assert_eq!(found, archive);
    assert_eq!(found.launch_date.to_rfc3339(), "2022-01-07T12:11:23+00:00");
    assert_eq!(
        found.deployment_date.to_utc().to_rfc3339(),
        "2022-01-07T12:11:23+00:00"
    );

    Ok(())
}
