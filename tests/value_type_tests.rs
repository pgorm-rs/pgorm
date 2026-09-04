#![allow(unused_imports, dead_code)]

pub mod common;

use std::sync::Arc;
use std::vec;

pub use common::{
    TestContext,
    features::{
        value_type::{Boolbean, Integer, StringVec, value_type_general, value_type_pg},
        *,
    },
    setup::*,
};
use pgorm::{ActiveValue::Unchanged, DatabaseConnection, entity::prelude::*, entity::*, set};
use pgorm_query::{ArrayType, ColumnType, Value, ValueType, ValueTypeError};
use pretty_assertions::assert_eq;

#[pgorm_macros::test]
async fn main() -> Result<(), Error> {
    let ctx = TestContext::new("value_type_tests").await;
    create_tables(&ctx.db).await?;
    let db = ctx.db.get().await?;
    insert_value(&db).await?;
    drop(db);
    ctx.delete().await;

    let ctx = TestContext::new("value_type_postgres_tests").await;
    create_tables(&ctx.db).await?;
    let db = ctx.db.get().await?;
    postgres_insert_value(&db).await?;
    drop(db);
    ctx.delete().await;

    type_test();
    conversion_test();

    Ok(())
}

pub async fn insert_value(db: &DatabaseConnection) -> Result<(), Error> {
    let model = value_type_general::Model {
        id: 1,
        number: 48.into(),
    };
    let result = model.clone().into_active_model().insert(db).await?;
    assert_eq!(result, model);

    Ok(())
}

pub async fn postgres_insert_value(db: &DatabaseConnection) -> Result<(), Error> {
    let model = value_type_pg::Model {
        id: 1,
        number: 48.into(),
        str_vec: StringVec(vec!["ab".to_string(), "cd".to_string()]),
    };
    let result = model.clone().into_active_model().insert(db).await?;
    assert_eq!(result, model);

    Ok(())
}

pub fn type_test() {
    assert_eq!(StringVec::type_name(), "StringVec");

    // custom types
    assert_eq!(Integer::array_type(), ArrayType::Int);
    assert_eq!(Integer::array_type(), ArrayType::Int);
    assert_eq!(Boolbean::column_type(), ColumnType::Boolean);
    assert_eq!(Boolbean::array_type(), ArrayType::Bool);

    // self implied
    assert_eq!(
        StringVec::column_type(),
        ColumnType::Array(Arc::new(ColumnType::String(StringLen::None)))
    );
    assert_eq!(StringVec::array_type(), ArrayType::String);
}

pub fn conversion_test() {
    let stringvec = StringVec(vec!["ab".to_string(), "cd".to_string()]);
    let string: Value = stringvec.into();
    assert_eq!(
        string,
        Value::Array(
            ArrayType::String,
            Some(Box::new(vec![
                "ab".to_string().into(),
                "cd".to_string().into()
            ]))
        )
    );

    let value_random_int = Value::Int(Some(523));
    let try_from_int =
        <Integer as ValueType>::try_from(value_random_int).expect("should be ok to convert");

    let direct_int: Integer = 523.into();
    assert_eq!(direct_int, try_from_int);

    // test for error
    let try_from_string_vec = <StringVec as ValueType>::try_from(Value::Char(Some('a')))
        .expect_err("should not be ok to convert char to stringvec");
    assert_eq!(try_from_string_vec.to_string(), ValueTypeError.to_string());
}
