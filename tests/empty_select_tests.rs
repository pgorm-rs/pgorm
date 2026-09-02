#![allow(unused_imports, dead_code)]

//! The execution-boundary guard on an empty select list.
//!
//! Run the test locally:
//! cargo test --test empty_select_tests

pub mod common;

pub use common::{TestContext, bakery_chain::*, setup::*};
use pgorm::{
    ActiveValue::Set, DatabaseConnection, DbErr, PaginatorTrait, QuerySelect, RuntimeErr,
    entity::prelude::*,
};
use pretty_assertions::assert_eq;

#[pgorm_macros::test]
async fn main() {
    let ctx = TestContext::new("bakery_chain_empty_select_tests").await;
    create_tables(&ctx.db).await.unwrap();

    let db = ctx.db.get().await.unwrap();
    empty_select_list_is_refused(&db).await;
    populated_select_list_still_runs(&db).await;

    drop(db);
    ctx.delete().await;
}

/// A `DbErr::Query` — rather than a `DbErr::Postgres` carrying `42601` — is the
/// evidence the statement never left the process.
fn assert_empty_select_list(err: DbErr) {
    let DbErr::Query(RuntimeErr::Internal(message)) = &err else {
        panic!("expected DbErr::Query(RuntimeErr::Internal(..)), got {err:?}");
    };
    assert_eq!(
        message,
        "select list is empty; add at least one column or expression"
    );
    assert_eq!(
        err.to_string(),
        "Query Error: select list is empty; add at least one column or expression"
    );
}

// [spec:pgorm:sem:query.build.modifiers+1/test]    every execution path over a
// statement whose projection list is empty returns DbErr::Query before the
// statement is sent
pub async fn empty_select_list_is_refused(db: &DatabaseConnection) {
    assert_empty_select_list(
        Bakery::find()
            .select_only()
            .all(db)
            .await
            .expect_err("all over an empty select list"),
    );

    assert_empty_select_list(
        Bakery::find()
            .select_only()
            .one(db)
            .await
            .expect_err("one over an empty select list"),
    );

    assert_empty_select_list(
        Bakery::find()
            .select_only()
            .one_opt(db)
            .await
            .expect_err("one_opt over an empty select list"),
    );

    assert_empty_select_list(
        Bakery::find()
            .select_only()
            .stream(db)
            .await
            .err()
            .expect("stream over an empty select list"),
    );

    assert_empty_select_list(
        Bakery::find()
            .select_only()
            .into_tuple::<i32>()
            .all(db)
            .await
            .expect_err("into_tuple over an empty select list"),
    );

    assert_empty_select_list(
        Bakery::find()
            .find_also_related(Baker)
            .select_only()
            .all(db)
            .await
            .expect_err("select-two over an empty select list"),
    );

    assert_empty_select_list(
        Bakery::find()
            .select_only()
            .paginate(db, 10)
            .fetch()
            .await
            .expect_err("fetch over an empty select list"),
    );

    assert_empty_select_list(
        Bakery::find()
            .select_only()
            .count(db)
            .await
            .expect_err("count over an empty select list"),
    );

    assert_empty_select_list(
        Bakery::find()
            .select_only()
            .cursor_by(bakery::Column::Id)
            .first(10)
            .all(db)
            .await
            .expect_err("cursor over an empty select list"),
    );
}

// [spec:pgorm:sem:query.build.modifiers+1/test]    a statement whose projection
// list is non-empty is untouched by the guard
pub async fn populated_select_list_still_runs(db: &DatabaseConnection) {
    Bakery::insert(bakery::ActiveModel {
        name: Set("SeaSide Bakery".to_owned()),
        profit_margin: Set(10.4),
        ..Default::default()
    })
    .exec(db)
    .await
    .expect("insert one bakery");

    let names: Vec<String> = Bakery::find()
        .select_only()
        .column(bakery::Column::Name)
        .into_tuple()
        .all(db)
        .await
        .expect("all over a re-populated select list");
    assert_eq!(names, ["SeaSide Bakery".to_owned()]);

    let counted = Bakery::find()
        .select_only()
        .column(bakery::Column::Id)
        .count(db)
        .await
        .expect("count over a re-populated select list");
    assert_eq!(counted, 1);

    let paged = Bakery::find()
        .select_only()
        .column(bakery::Column::Id)
        .into_tuple::<i32>()
        .paginate(db, 10)
        .fetch()
        .await
        .expect("fetch over a re-populated select list");
    assert_eq!(paged, [1]);

    let cursored = Bakery::find()
        .select_only()
        .columns([
            bakery::Column::Id,
            bakery::Column::Name,
            bakery::Column::ProfitMargin,
        ])
        .cursor_by(bakery::Column::Id)
        .first(10)
        .all(db)
        .await
        .expect("cursor over a re-populated select list");
    assert_eq!(cursored.len(), 1);
}
