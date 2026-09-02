//! The compile-time-checked literal at the raw-SQL escape hatches it exists for.
//!
//! Every statement here was held to the PostgreSQL grammar while this file was
//! compiled; the assertions are that the same text then runs against a live
//! server unchanged.
#![cfg(feature = "sql-macro")]
#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::TestContext;
use futures::TryStreamExt;
use pgorm::{
    ConnectionTrait, DbErr, FromQueryResult, SelectModel, SelectorRaw, TransactionTrait, sql,
};
use pgorm_query::Values;
use pretty_assertions::assert_eq;

const NO_PARAMS: [&(dyn tokio_postgres::types::ToSql + Sync); 0] = [];

#[derive(FromQueryResult)]
struct Cake {
    id: i32,
    name: String,
}

const SCHEMA: &str = sql!(
    "CREATE TABLE cake (id int primary key, name text not null);
     INSERT INTO cake (id, name) VALUES (1, 'Chocolate'), (2, 'Lemon');"
);

// [spec:pgorm:def:macros.sql/test]    the literal reaches the server unchanged
// [spec:pgorm:sem:macros.sql.script/test]    a validated script through `batch_execute`
#[pgorm_macros::test]
async fn checked_literal_drives_selector_raw() -> Result<(), DbErr> {
    let ctx = TestContext::new("sql_macro_selector_raw").await;
    let db = ctx.db.get().await?;

    db.batch_execute(SCHEMA).await?;

    let rows = SelectorRaw::<SelectModel<Cake>>::from_statement::<Cake>(
        sql!(r#"SELECT "id", "name" FROM "cake" ORDER BY "id""#).to_owned(),
        Values(Vec::new()),
    )
    .all(&db)
    .await?;

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, 1);
    assert_eq!(rows[0].name, "Chocolate");
    assert_eq!(rows[1].name, "Lemon");

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:def:macros.sql/test]    the same literal bound as a prepared statement
#[pgorm_macros::test]
async fn checked_literal_drives_query_raw() -> Result<(), DbErr> {
    let ctx = TestContext::new("sql_macro_query_raw").await;
    let db = ctx.db.get().await?;

    db.batch_execute(SCHEMA).await?;

    let affected = db
        .execute_raw(sql!("DELETE FROM cake WHERE name = $1"), [&"Lemon"])
        .await?;
    assert_eq!(affected, 1);

    let mut stream = Box::pin(
        db.query_raw(sql!("SELECT id, name FROM cake ORDER BY id"), NO_PARAMS)
            .await?,
    );

    let row = stream.try_next().await?.ok_or(DbErr::RecordNotFound)?;
    assert_eq!(row.get::<_, i32>(0), 1);
    assert_eq!(row.get::<_, &str>(1), "Chocolate");

    assert!(stream.try_next().await?.is_none());

    drop(stream);
    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:req:macros.sql.ceiling/test]    grammar, not schema: a statement the
// macro accepts can still fail against a real catalog
#[pgorm_macros::test]
async fn checked_literal_can_still_fail_at_runtime() -> Result<(), DbErr> {
    let ctx = TestContext::new("sql_macro_unknown_relation").await;
    let db = ctx.db.get().await?;

    db.execute_raw(sql!("SELECT nowhere FROM no_such_table"), NO_PARAMS)
        .await
        .expect_err("the grammar has no catalog; the server does");

    drop(db);
    ctx.delete().await;

    Ok(())
}
