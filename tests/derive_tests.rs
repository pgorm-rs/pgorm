#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::TestContext;
use pgorm::{DbErr, FromQueryResult, SelectModel, SelectorRaw, TryGetable};
use pgorm_query::Values;

#[derive(FromQueryResult)]
struct SimpleTest {
    _foo: i32,
    _bar: String,
}

#[derive(FromQueryResult)]
struct GenericTest<T: TryGetable> {
    _foo: i32,
    _bar: T,
}

#[derive(FromQueryResult)]
struct DoubleGenericTest<T: TryGetable, F: TryGetable> {
    _foo: T,
    _bar: F,
}

#[derive(FromQueryResult)]
struct BoundsGenericTest<T: TryGetable + Copy + Clone + 'static> {
    _foo: T,
}

#[derive(FromQueryResult)]
struct WhereGenericTest<T>
where
    T: TryGetable + Copy + Clone + 'static,
{
    _foo: T,
}

#[derive(FromQueryResult)]
struct AlreadySpecifiedBoundsGenericTest<T: TryGetable> {
    _foo: T,
}

#[derive(FromQueryResult)]
struct MixedGenericTest<T: TryGetable + Clone, F>
where
    F: TryGetable + Copy + Clone + 'static,
{
    _foo: T,
    _bar: F,
}

trait MyTrait {
    type Item: TryGetable;
}

#[derive(FromQueryResult)]
struct TraitAssociateTypeTest<T>
where
    T: MyTrait,
{
    _foo: T::Item,
}

#[derive(FromQueryResult)]
struct FromQueryAttributeTests {
    #[pgorm(skip)]
    _foo: i32,
    _bar: String,
}

fn raw<M>(sql: &str) -> SelectorRaw<SelectModel<M>>
where
    M: FromQueryResult,
{
    SelectorRaw::<SelectModel<M>>::from_statement::<M>(sql.to_owned(), Values(Vec::new()))
}

// [spec:pgorm:def:exec.crud/test]    `SelectorRaw::from_statement` decoding
// through `SelectModel`, i.e. `SelectorTrait::from_raw_query_result` with an
// empty prefix
#[pgorm_macros::test]
async fn from_query_result_derive() -> Result<(), DbErr> {
    let ctx = TestContext::new("derive_tests_from_query_result").await;
    let db = ctx.db.get().await?;

    let row = raw::<SimpleTest>(r#"SELECT 1 AS "_foo", 'one' AS "_bar""#)
        .one(&db)
        .await?;
    assert_eq!(row._foo, 1);
    assert_eq!(row._bar, "one");

    let row = raw::<GenericTest<String>>(r#"SELECT 2 AS "_foo", 'two' AS "_bar""#)
        .one(&db)
        .await?;
    assert_eq!(row._foo, 2);
    assert_eq!(row._bar, "two");

    let row =
        raw::<DoubleGenericTest<bool, f64>>(r#"SELECT true AS "_foo", 1.5::float8 AS "_bar""#)
            .one(&db)
            .await?;
    assert!(row._foo);
    assert_eq!(row._bar, 1.5);

    // A skipped field is never read from the row; it takes `Default::default()`.
    let row = raw::<FromQueryAttributeTests>(r#"SELECT 'three' AS "_bar""#)
        .one(&db)
        .await?;
    assert_eq!(row._foo, 0);
    assert_eq!(row._bar, "three");

    drop(db);
    ctx.delete().await;

    Ok(())
}
