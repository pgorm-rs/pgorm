#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, bakery_chain::*, setup::*};
use futures::TryStreamExt;
use pgorm::{
    ActiveValue::Set, DatabaseConnection, ItemsAndPagesNumber, PaginatorTrait, QueryOrder,
    QuerySelect, entity::prelude::*,
};
use pgorm_query::{Value, Values};
use pretty_assertions::assert_eq;

const BAKERIES: [(&str, f64); 7] = [
    ("Alpha Bakery", 1.0),
    ("Bravo Bakery", 2.0),
    ("Charlie Bakery", 3.0),
    ("Delta Bakery", 4.0),
    ("Echo Bakery", 5.0),
    ("Foxtrot Bakery", 6.0),
    ("Golf Bakery", 7.0),
];

async fn seed(db: &DatabaseConnection) -> Result<(), DbErr> {
    for (name, margin) in BAKERIES {
        bakery::ActiveModel {
            name: Set(name.to_owned()),
            profit_margin: Set(margin),
            ..Default::default()
        }
        .insert(db)
        .await?;
    }

    Ok(())
}

fn names(models: &[bakery::Model]) -> Vec<&str> {
    models.iter().map(|m| m.name.as_str()).collect()
}

const RAW_ALL: &str = r#"SELECT "id", "name", "profit_margin" FROM "bakery" ORDER BY "id" ASC"#;

// [spec:pgorm:def:exec.paginator/test]    paginate is reachable from every
// selector shape, and `count` is `paginate(db, 1).num_items()`
// [spec:pgorm:sem:exec.paginator.fetch/test]    zero-indexed pages, an
// independent page cursor, and `next` advancing without fetching
#[pgorm_macros::test]
async fn paginator_fetch_page() -> Result<(), DbErr> {
    let ctx = TestContext::new("paginator_tests_fetch_page").await;
    create_tables(&ctx.db).await?;
    let db = ctx.db.get().await?;
    seed(&db).await?;

    let mut paginator = Bakery::find()
        .order_by_asc(bakery::Column::Id)
        .paginate(&db, 3);

    // Pages are zero-indexed and the trailing page is partial.
    assert_eq!(
        names(&paginator.fetch_page(0).await?),
        ["Alpha Bakery", "Bravo Bakery", "Charlie Bakery"]
    );
    assert_eq!(
        names(&paginator.fetch_page(1).await?),
        ["Delta Bakery", "Echo Bakery", "Foxtrot Bakery"]
    );
    assert_eq!(names(&paginator.fetch_page(2).await?), ["Golf Bakery"]);
    assert!(paginator.fetch_page(3).await?.is_empty());

    // `fetch_page` neither consults nor advances the paginator's own cursor.
    assert_eq!(paginator.cur_page(), 0);
    assert_eq!(
        names(&paginator.fetch().await?),
        names(&paginator.fetch_page(0).await?)
    );

    // `next` increments without fetching; `fetch` then reads the new page.
    paginator.next();
    assert_eq!(paginator.cur_page(), 1);
    assert_eq!(
        names(&paginator.fetch().await?),
        ["Delta Bakery", "Echo Bakery", "Foxtrot Bakery"]
    );
    paginator.next();
    paginator.next();
    assert_eq!(paginator.cur_page(), 3);
    assert!(paginator.fetch().await?.is_empty());

    // `PaginatorTrait` is implemented for `Selector<S>` too.
    let names_only: Vec<String> = Bakery::find()
        .select_only()
        .column(bakery::Column::Name)
        .order_by_asc(bakery::Column::Id)
        .into_tuple::<String>()
        .paginate(&db, 2)
        .fetch_page(1)
        .await?;
    assert_eq!(names_only, ["Charlie Bakery", "Delta Bakery"]);

    // ... and for `SelectTwo<E, F>`, via `into_model`.
    let joined: Vec<(bakery::Model, Option<baker::Model>)> = Bakery::find()
        .find_also_related(Baker)
        .order_by_asc(bakery::Column::Id)
        .paginate(&db, 2)
        .fetch_page(0)
        .await?;
    assert_eq!(joined.len(), 2);
    assert_eq!(joined[0].1, None);

    // `count` is defined as `paginate(db, 1).num_items()`.
    assert_eq!(Bakery::find().count(&db).await?, 7);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:exec.paginator.count/test]    counting strips limit, offset
// and ORDER BY; page count is ceiling division and is never cached
#[pgorm_macros::test]
async fn paginator_count() -> Result<(), DbErr> {
    let ctx = TestContext::new("paginator_tests_count").await;
    create_tables(&ctx.db).await?;
    let db = ctx.db.get().await?;

    // Zero items yield zero pages.
    let empty = Bakery::find().paginate(&db, 3);
    assert_eq!(empty.num_items().await?, 0);
    assert_eq!(empty.num_pages().await?, 0);

    seed(&db).await?;

    // A partial trailing page still counts as a page: 7 items / 3 == 3 pages.
    let paginator = Bakery::find()
        .order_by_asc(bakery::Column::Id)
        .paginate(&db, 3);
    assert_eq!(paginator.num_items().await?, 7);
    assert_eq!(paginator.num_pages().await?, 3);

    let ItemsAndPagesNumber {
        number_of_items,
        number_of_pages,
    } = paginator.num_items_and_pages().await?;
    assert_eq!(number_of_items, 7);
    assert_eq!(number_of_pages, 3);

    // An exact multiple does not gain a trailing page.
    assert_eq!(Bakery::find().paginate(&db, 7).num_pages().await?, 1);
    assert_eq!(Bakery::find().paginate(&db, 1).num_pages().await?, 7);

    // The count subquery drops limit, offset and ORDER BY, so a query that
    // would return two rows still counts every matching row.
    let windowed = Bakery::find()
        .order_by_desc(bakery::Column::Id)
        .limit(2)
        .offset(1)
        .paginate(&db, 3);
    assert_eq!(windowed.fetch_page(0).await?.len(), 3);
    assert_eq!(windowed.num_items().await?, 7);

    // Counts are re-run, not cached: a row inserted after the first count is
    // visible to the second.
    bakery::ActiveModel {
        name: Set("Hotel Bakery".to_owned()),
        profit_margin: Set(8.0),
        ..Default::default()
    }
    .insert(&db)
    .await?;
    assert_eq!(paginator.num_items().await?, 8);
    assert_eq!(paginator.num_pages().await?, 3);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:exec.paginator.iterate/test]    `fetch_and_next` terminates
// only on an empty page, and `into_stream` wraps that same loop
#[pgorm_macros::test]
async fn paginator_iterate() -> Result<(), DbErr> {
    let ctx = TestContext::new("paginator_tests_iterate").await;
    create_tables(&ctx.db).await?;
    let db = ctx.db.get().await?;
    seed(&db).await?;

    let mut paginator = Bakery::find()
        .order_by_asc(bakery::Column::Id)
        .paginate(&db, 3);

    let mut pages = Vec::new();
    while let Some(page) = paginator.fetch_and_next().await? {
        pages.push(page.len());
    }
    assert_eq!(pages, [3, 3, 1]);
    // The counter is advanced by the empty fetch that ended the loop too.
    assert_eq!(paginator.cur_page(), 4);

    // A result set that is an exact multiple of `page_size` costs one extra
    // query returning zero rows before termination is detected.
    let mut exact = Bakery::find()
        .order_by_asc(bakery::Column::Id)
        .paginate(&db, 7);
    assert_eq!(exact.fetch_and_next().await?.map(|p| p.len()), Some(7));
    assert_eq!(exact.fetch_and_next().await?.map(|p| p.len()), None);
    assert_eq!(exact.cur_page(), 2);

    // `into_stream` yields one item per non-empty page and then ends.
    let streamed: Vec<Vec<bakery::Model>> = Bakery::find()
        .order_by_asc(bakery::Column::Id)
        .paginate(&db, 3)
        .into_stream()
        .try_collect()
        .await?;
    assert_eq!(
        streamed.iter().map(|p| p.len()).collect::<Vec<_>>(),
        [3, 3, 1]
    );
    assert_eq!(
        names(&streamed.concat()),
        BAKERIES.map(|(name, _)| name).to_vec()
    );

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:req:exec.paginator.page-size/test]    a zero page size panics
// rather than returning a `DbErr`, on both `paginate` implementations
#[pgorm_macros::test]
async fn paginator_rejects_zero_page_size() -> Result<(), DbErr> {
    let ctx = TestContext::new("paginator_tests_zero_page_size").await;
    create_tables(&ctx.db).await?;
    let db = ctx.db.get().await?;

    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let selector = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = Bakery::find().paginate(&db, 0);
    }));

    let raw = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = Bakery::find()
            .from_raw_sql(RAW_ALL.to_owned(), Values(Vec::new()))
            .paginate(&db, 0);
    }));

    std::panic::set_hook(hook);

    for payload in [selector, raw] {
        let payload = payload.expect_err("a zero page size must panic");
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or_default()
            .to_owned();
        assert!(
            message.contains("page_size should not be zero"),
            "unexpected panic payload: {message}"
        );
    }

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:def:exec.crud/test]    `Select::from_raw_sql` builds a
// `SelectorRaw` from a raw statement plus `Values`
// [spec:pgorm:sem:exec.paginator.raw/test]    the raw statement is re-embedded
// as a custom expression after its `SELECT` keyword is sliced off
#[pgorm_macros::test]
async fn paginator_raw() -> Result<(), DbErr> {
    let ctx = TestContext::new("paginator_tests_raw").await;
    create_tables(&ctx.db).await?;
    let db = ctx.db.get().await?;
    seed(&db).await?;

    // No bind values: the remainder is embedded with `Expr::cust`.
    let mut paginator = Bakery::find()
        .from_raw_sql(RAW_ALL.to_owned(), Values(Vec::new()))
        .paginate(&db, 3);

    assert_eq!(
        names(&paginator.fetch_page(0).await?),
        ["Alpha Bakery", "Bravo Bakery", "Charlie Bakery"]
    );
    assert_eq!(names(&paginator.fetch_page(2).await?), ["Golf Bakery"]);
    assert_eq!(paginator.num_items().await?, 7);
    assert_eq!(paginator.num_pages().await?, 3);
    assert_eq!(paginator.fetch_and_next().await?.map(|p| p.len()), Some(3));

    // With bind values: the remainder is embedded with `Expr::cust_with_values`
    // and the `$1` marker keeps its value.
    let filtered = Bakery::find()
        .from_raw_sql(
            r#"SELECT "id", "name", "profit_margin" FROM "bakery" WHERE "profit_margin" > $1 ORDER BY "id" ASC"#
                .to_owned(),
            Values(vec![Value::Double(Some(4.0))]),
        )
        .paginate(&db, 2);

    assert_eq!(filtered.num_items().await?, 3);
    assert_eq!(
        names(&filtered.fetch_page(0).await?),
        ["Echo Bakery", "Foxtrot Bakery"]
    );
    assert_eq!(names(&filtered.fetch_page(1).await?), ["Golf Bakery"]);

    // Leading whitespace is trimmed before the keyword is sliced.
    let padded = Bakery::find()
        .from_raw_sql(format!("   {RAW_ALL}"), Values(Vec::new()))
        .paginate(&db, 3);
    assert_eq!(padded.num_items().await?, 7);

    // No validation is performed on the sliced prefix: a statement that does
    // not begin with `SELECT` is mangled into invalid SQL rather than rejected.
    let mangled = Bakery::find()
        .from_raw_sql(
            r#"WITH t AS (SELECT "id", "name", "profit_margin" FROM "bakery") SELECT * FROM t"#
                .to_owned(),
            Values(Vec::new()),
        )
        .paginate(&db, 3)
        .fetch_page(0)
        .await;
    assert!(
        matches!(mangled, Err(DbErr::Postgres(_))),
        "unexpected result: {mangled:?}"
    );

    drop(db);
    ctx.delete().await;

    Ok(())
}
