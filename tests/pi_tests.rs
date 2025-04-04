#![allow(unused_imports, dead_code)]

pub mod common;

use common::{TestContext, features::*, setup::*};
use pgorm::{DatabasePool, entity::prelude::*, entity::*};
use pretty_assertions::assert_eq;
use std::str::FromStr;

#[pgorm_macros::test]
async fn main() -> Result<(), DbErr> {
    let ctx = TestContext::new("pi_tests").await;
    create_tables(&ctx.db).await?;
    create_and_update_pi(&ctx.db).await?;
    ctx.delete().await;

    Ok(())
}

pub async fn create_and_update_pi(db: &DatabasePool) -> Result<(), DbErr> {
    let pi = pi::Model {
        id: 1,
        decimal: rust_dec(3.1415926536),
        big_decimal: BigDecimal::from_str("3.1415926536").unwrap(),
        decimal_opt: None,
        big_decimal_opt: None,
    };

    let res = pi.clone().into_active_model().insert(db).await?;

    let model = Pi::find().one(db).await?;
    assert_eq!(model, Some(res));
    assert_eq!(model, Some(pi.clone()));

    let res = pi::ActiveModel {
        decimal_opt: Set(Some(rust_dec(3.1415926536))),
        big_decimal_opt: Set(Some(BigDecimal::from_str("3.1415926536").unwrap())),
        ..pi.clone().into_active_model()
    }
    .update(db)
    .await?;

    let model = Pi::find().one(db).await?;
    assert_eq!(model, Some(res));
    assert_eq!(
        model,
        Some(pi::Model {
            id: 1,
            decimal: rust_dec(3.1415926536),
            big_decimal: BigDecimal::from_str("3.1415926536").unwrap(),
            decimal_opt: Some(rust_dec(3.1415926536)),
            big_decimal_opt: Some(BigDecimal::from_str("3.1415926536").unwrap()),
        })
    );

    Ok(())
}
