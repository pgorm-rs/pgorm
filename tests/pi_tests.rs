#![allow(unused_imports, dead_code)]

pub mod common;

use common::{TestContext, features::*, setup::*};
use pgorm::{ActiveValue::Set, DatabaseConnection, entity::prelude::*, entity::*};
use pretty_assertions::assert_eq;

#[pgorm_macros::test]
async fn main() -> Result<(), DbErr> {
    let ctx = TestContext::new("pi_tests").await;
    create_tables(&ctx.db).await?;

    let db = ctx.db.get().await?;
    create_and_update_pi(&db).await?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

pub async fn create_and_update_pi(db: &DatabaseConnection) -> Result<(), DbErr> {
    let pi = pi::Model {
        id: 1,
        decimal: rust_dec(3.1415926536),
        decimal_opt: None,
    };

    let res = pi.clone().into_active_model().insert(db).await?;

    let model = Pi::find().one_opt(db).await?;
    assert_eq!(model, Some(res));
    assert_eq!(model, Some(pi.clone()));

    let res = pi::ActiveModel {
        decimal_opt: Set(Some(rust_dec(3.1415926536))),
        ..pi.clone().into_active_model()
    }
    .update(db)
    .await?;

    let model = Pi::find().one_opt(db).await?;
    assert_eq!(model, Some(res));
    assert_eq!(
        model,
        Some(pi::Model {
            id: 1,
            decimal: rust_dec(3.1415926536),
            decimal_opt: Some(rust_dec(3.1415926536)),
        })
    );

    Ok(())
}
