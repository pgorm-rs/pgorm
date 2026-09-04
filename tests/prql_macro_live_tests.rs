//! `prql!`-produced statements against a live server, with bound parameters.
//!
//! The macro's own suite proves the SQL text and the Values shape; what it
//! cannot prove is that the placeholders and their values meet again at the
//! server — that `$1` reused in two clauses binds one value in both, and
//! that the rows coming back decode. Those are semantics, so they are
//! asserted against PostgreSQL with decoded rows.
//!
//! Run the test locally:
//! DATABASE_URL=postgres://postgres:postgres@127.0.0.1:54329 cargo test --test prql_macro_live_tests
#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::TestContext;
use pgorm::{
    ConnectionTrait, DecodeRaw, Error, FromQueryResult, SelectModel, SelectorRaw, Values, prql, sql,
};
use pretty_assertions::assert_eq;

#[derive(FromQueryResult, Debug, PartialEq)]
struct Invoice {
    id: i32,
    billing_city: String,
    total: i64,
}

const SCHEMA: &str = sql!(
    "CREATE TABLE invoice (id int primary key, billing_city text not null, total bigint not null);
     INSERT INTO invoice (id, billing_city, total) VALUES
         (1, 'Berlin', 50), (2, 'Berlin', 120), (3, 'Oslo', 200), (4, 'Oslo', 80);"
);

fn selector(sql: &str, values: Values) -> SelectorRaw<SelectModel<Invoice>> {
    (sql, values).into_model::<Invoice>()
}

// [spec:pgorm:def:macros.prql/test]    the expansion binds its argument end to end
#[pgorm_macros::test]
async fn bound_param_reaches_the_server() -> Result<(), Error> {
    let ctx = TestContext::new("prql_macro_bound_param").await;
    let db = ctx.db.get().await?;
    db.batch_execute(SCHEMA).await?;

    let (query, values) = prql!("from invoice | filter total > $1 | sort id", 100_i64);
    let rows = selector(query, values).all(&db).await?;

    assert_eq!(
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![2, 3]
    );

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:macros.prql.census/test]    `$1` twice, one value, both clauses
#[pgorm_macros::test]
async fn reused_placeholder_binds_once_live() -> Result<(), Error> {
    let ctx = TestContext::new("prql_macro_reused_placeholder").await;
    let db = ctx.db.get().await?;
    db.batch_execute(SCHEMA).await?;

    let (query, values) = prql!(
        "from invoice | filter total > $1 | filter id != $1 | sort id",
        100_i64,
    );
    assert_eq!(query.matches("$1").count(), 2);
    assert_eq!(values.0.len(), 1);

    // total > 100 keeps ids 2 and 3; id != 100 excludes nothing.
    let rows = selector(query, values).all(&db).await?;
    assert_eq!(
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![2, 3]
    );

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:def:macros.prql/test]    two placeholders, two values, right slots
#[pgorm_macros::test]
async fn arguments_land_in_placeholder_order() -> Result<(), Error> {
    let ctx = TestContext::new("prql_macro_placeholder_order").await;
    let db = ctx.db.get().await?;
    db.batch_execute(SCHEMA).await?;

    let city = "Oslo".to_owned();
    let (query, values) = prql!(
        "from invoice | filter billing_city == $1 | filter total < $2 | sort id",
        city,
        150_i64,
    );
    let rows = selector(query, values).all(&db).await?;

    assert_eq!(
        rows,
        vec![Invoice {
            id: 4,
            billing_city: "Oslo".to_owned(),
            total: 80,
        }]
    );

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:macros.prql.sstring/test]    an s-string's SQL runs as written
#[pgorm_macros::test]
async fn sstring_survives_to_execution() -> Result<(), Error> {
    let ctx = TestContext::new("prql_macro_sstring_live").await;
    let db = ctx.db.get().await?;
    db.batch_execute(SCHEMA).await?;

    #[derive(FromQueryResult)]
    struct City {
        lowered: String,
    }

    let (query, values) = prql!(
        r#"from invoice | filter total > $1 | derive lowered = s"lower(billing_city)" | select {lowered}"#,
        150_i64,
    );
    let rows = (query, values).into_model::<City>().all(&db).await?;

    assert_eq!(
        rows.iter()
            .map(|row| row.lowered.as_str())
            .collect::<Vec<_>>(),
        vec!["oslo"]
    );

    drop(db);
    ctx.delete().await;

    Ok(())
}
