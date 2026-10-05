#![allow(unused_imports, dead_code)]

//! Range and multirange values against a live PostgreSQL server.
//!
//! The unit tests hold the codec to the bytes it writes and reads. What only a
//! server settles is what those bytes mean: that every built-in range type
//! stores what was bound and reads it back, that a discrete range comes back
//! in the server's canonical form, that the empty range and an unbounded side
//! are told apart from each other and from a subtype's own `infinity`, that a
//! multirange is merged, that the inline and the bound renderings of one value
//! are one value, and what the server refuses.

pub mod common;
pub use common::{TestContext, setup::*};
use pgorm::pgorm_query::{
    BinOper, ColumnDef, ColumnType, Expr, Name, Query, SimpleExpr, Table, Value, Values, alias,
    extension::{RangeDefinition, Type},
};
use pgorm::{
    ConnectionTrait, DecodeRaw, QuerySelect, Schema, TryGetable, TryGetableMany, entity::prelude::*,
};
use pretty_assertions::assert_eq;
use std::ops::Bound::{self, Excluded, Included, Unbounded};
use tokio_postgres::error::SqlState;

mod spans {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "spans")]
    pub struct Model {
        #[pgorm(primary_key, auto_increment = false)]
        pub id: i32,
        pub counts: Range<i32>,
        pub bigs: Range<i64>,
        pub amounts: Range<Decimal>,
        pub days: Range<Date>,
        pub stamps: Range<DateTime>,
        pub instants: Range<DateTimeWithTimeZone>,
        pub count_sets: Multirange<i32>,
        pub day_sets: Multirange<Date>,
        pub maybe: Option<Range<i32>>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

#[pgorm_macros::test]
async fn main() -> Result<(), Error> {
    let ctx = TestContext::new("range_value_tests").await;
    let db = ctx.db.get().await?;

    let schema = Schema::new();
    create_table_without_asserts(&db, &schema.create_table_from_entity(spans::Entity)).await?;

    every_range_type_round_trips_through_an_entity(&db).await?;
    a_discrete_range_comes_back_canonical(&db).await?;
    the_empty_range_is_not_every_value(&db).await?;
    an_infinite_bound_is_not_unbounded(&db).await?;
    a_multirange_comes_back_merged(&db).await?;
    the_bound_and_inline_renderings_agree(&db).await?;
    the_operators_read_ranges(&db).await?;
    a_created_range_type_binds_by_its_subtype(&db).await?;
    the_server_refuses_what_it_cannot_store(&db).await?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

fn date(y: i16, m: i8, d: i8) -> Date {
    jiff::civil::date(y, m, d)
}

fn stamp(y: i16, m: i8, d: i8, h: i8) -> DateTime {
    date(y, m, d).at(h, 0, 0, 0)
}

fn instant(second: i64, micros: i32) -> DateTimeWithTimeZone {
    jiff::Timestamp::new(second, micros * 1_000).expect("an instant in range")
}

fn dec(units: i64, scale: u32) -> Decimal {
    Decimal::new(units, scale)
}

/// Bind `value` as the statement's one parameter and decode the one column
/// the statement returns.
async fn read<T>(db: &DatabaseConnection, sql: &str, value: Value) -> Result<T, Error>
where
    T: TryGetable,
{
    (sql, Values(vec![value])).into_tuple().one(db).await
}

fn refused_with(error: &Error, state: &SqlState) {
    match error {
        Error::Postgres(e) => assert_eq!(e.code(), Some(state), "{e}"),
        other => panic!("expected Error::Postgres, got {other:?}"),
    }
}

/// A model whose discrete ranges are already canonical, so what is read back
/// is what was written.
fn model(id: i32) -> spans::Model {
    spans::Model {
        id,
        counts: Range::from(1..6),
        bigs: Range::from(-5_000_000_000i64..),
        amounts: Range::new(Excluded(dec(0, 2)), Included(dec(150, 2))),
        days: Range::from(date(2024, 2, 1)..date(2024, 3, 1)),
        stamps: Range::from(..=stamp(2024, 2, 29, 23)),
        instants: Range::from(instant(1_700_000_000, 123_456)..instant(1_800_000_000, 0)),
        count_sets: [Range::from(1..4), Range::from(5..8)].into_iter().collect(),
        day_sets: Multirange::default(),
        maybe: None,
    }
}

/// Every built-in range type is written by an entity insert and read back by
/// its select, a NULL range included; a numeric bound keeps its scale and a
/// timestamp bound its microseconds.
// [spec:pgorm:def:sql.value.range/test]
// [spec:pgorm:def:exec.decode.range/test]
// [spec:pgorm:req:exec.cursor.binding-range/test]
async fn every_range_type_round_trips_through_an_entity(
    db: &DatabaseConnection,
) -> Result<(), Error> {
    let first = model(1);
    let returned = first.clone().into_active_model().insert(db).await?;
    assert_eq!(returned, first);
    assert_eq!(spans::Entity::find_by_id(1).one(db).await?, first);

    let second = spans::Model {
        maybe: Some(Range::Empty),
        day_sets: [Range::from(date(2024, 1, 1)..)].into_iter().collect(),
        ..model(2)
    };
    second.clone().into_active_model().insert(db).await?;
    let read: Vec<(Range<Decimal>, Option<Range<i32>>)> = spans::Entity::find()
        .select([spans::Column::Amounts, spans::Column::Maybe])
        .order_by_asc(spans::Column::Id)
        .into_tuple()
        .all(db)
        .await?;
    assert_eq!(
        read,
        [
            (model(1).amounts, None),
            (second.amounts.clone(), Some(Range::Empty)),
        ]
    );
    let Some(Included(upper)) = read[0].0.upper().cloned() else {
        panic!("an inclusive upper bound");
    };
    assert_eq!(upper.scale(), 2);
    Ok(())
}

/// The server stores a discrete range in its canonical `[)` form, so a range
/// written with other brackets reads back as the same set spelled
/// differently; a continuous range is stored as written.
// [spec:pgorm:def:sql.value.range/test]
async fn a_discrete_range_comes_back_canonical(db: &DatabaseConnection) -> Result<(), Error> {
    let int4 = |range: Range<i32>| Value::from(range);
    assert_eq!(
        read::<Range<i32>>(db, "SELECT $1::int4range", int4(Range::from(1..=5))).await?,
        Range::from(1..6)
    );
    assert_eq!(
        read::<Range<i32>>(
            db,
            "SELECT $1::int4range",
            int4(Range::new(Excluded(1), Included(5)))
        )
        .await?,
        Range::from(2..6)
    );
    assert_eq!(
        read::<Range<i32>>(db, "SELECT $1::int4range", int4(Range::from(..=5))).await?,
        Range::from(..6)
    );
    assert_eq!(
        read::<Range<i64>>(
            db,
            "SELECT $1::int8range",
            Range::new(Excluded(1i64), Excluded(3)).into()
        )
        .await?,
        Range::from(2..3)
    );
    assert_eq!(
        read::<Range<Date>>(
            db,
            "SELECT $1::daterange",
            Range::from(date(2024, 1, 1)..=date(2024, 1, 31)).into()
        )
        .await?,
        Range::from(date(2024, 1, 1)..date(2024, 2, 1))
    );
    let numeric = Range::from(dec(0, 2)..=dec(15, 1));
    assert_eq!(
        read::<Range<Decimal>>(db, "SELECT $1::numrange", numeric.clone().into()).await?,
        numeric
    );
    let stamps = Range::new(
        Excluded(stamp(2024, 1, 1, 0)),
        Included(stamp(2024, 1, 2, 0)),
    );
    assert_eq!(
        read::<Range<DateTime>>(db, "SELECT $1::tsrange", stamps.clone().into()).await?,
        stamps
    );
    Ok(())
}

/// The empty range is one value: written as itself or as bounds that hold
/// nothing, it reads back as `Empty`, and it is never the range of every
/// value, which is two unbounded sides.
// [spec:pgorm:def:sql.value.range/test]
// [spec:pgorm:def:exec.decode.range/test]
// [spec:pgorm:req:exec.cursor.binding-range/test]
async fn the_empty_range_is_not_every_value(db: &DatabaseConnection) -> Result<(), Error> {
    let empty = Value::from(Range::<i32>::Empty);
    let everything = Value::from(Range::<i32>::from(..));
    assert_eq!(
        read::<Range<i32>>(db, "SELECT $1::int4range", empty.clone()).await?,
        Range::Empty
    );
    assert_eq!(
        read::<Range<i32>>(db, "SELECT $1::int4range", Range::from(5..5).into()).await?,
        Range::Empty
    );
    assert_eq!(
        read::<Range<i32>>(db, "SELECT $1::int4range", everything.clone()).await?,
        Range::from(..)
    );
    assert!(read::<bool>(db, "SELECT isempty($1::int4range)", empty).await?);
    assert!(!read::<bool>(db, "SELECT isempty($1::int4range)", everything.clone()).await?);
    assert!(read::<bool>(db, "SELECT lower_inf($1::int4range)", everything).await?);
    Ok(())
}

/// A subtype's own `infinity` is a bound value, not an absent bound: the
/// server keeps it inside the brackets and reports the side as finite, and
/// the decode refuses it as it refuses that value outside a range, rather
/// than reading it as unbounded.
// [spec:pgorm:def:exec.decode.range/test]
async fn an_infinite_bound_is_not_unbounded(db: &DatabaseConnection) -> Result<(), Error> {
    let unbounded: Range<Date> = read(
        db,
        "SELECT daterange($1::date, NULL, '[)')",
        date(2024, 1, 1).into(),
    )
    .await?;
    assert_eq!(unbounded, Range::from(date(2024, 1, 1)..));

    let infinite = Value::String(Some(Box::new("[2024-01-01,infinity)".to_owned())));
    assert!(
        !read::<bool>(
            db,
            "SELECT upper_inf($1::text::daterange)",
            infinite.clone()
        )
        .await?
    );
    let decoded = read::<Range<Date>>(db, "SELECT $1::text::daterange", infinite).await;
    assert!(decoded.is_err(), "an infinite bound decoded as {decoded:?}");

    let infinite = Value::String(Some(Box::new("[-infinity,infinity]".to_owned())));
    let decoded = read::<Range<DateTime>>(db, "SELECT $1::text::tsrange", infinite).await;
    assert!(decoded.is_err(), "an infinite bound decoded as {decoded:?}");
    Ok(())
}

/// A multirange is stored sorted, with overlapping and adjacent ranges merged
/// and empty ones dropped; the empty multirange is a value of its own.
// [spec:pgorm:def:sql.value.range/test]
// [spec:pgorm:def:exec.decode.range/test]
// [spec:pgorm:req:exec.cursor.binding-range/test]
async fn a_multirange_comes_back_merged(db: &DatabaseConnection) -> Result<(), Error> {
    let written: Multirange<i32> = [
        Range::from(5..8),
        Range::from(1..3),
        Range::from(2..4),
        Range::Empty,
    ]
    .into_iter()
    .collect();
    assert_eq!(
        read::<Multirange<i32>>(db, "SELECT $1::int4multirange", written.into()).await?,
        [Range::from(1..4), Range::from(5..8)].into_iter().collect()
    );
    assert_eq!(
        read::<Multirange<i32>>(
            db,
            "SELECT $1::int4multirange",
            Multirange::<i32>::default().into()
        )
        .await?,
        Multirange::default()
    );
    let instants: Multirange<DateTimeWithTimeZone> =
        [Range::from(instant(0, 1)..)].into_iter().collect();
    assert_eq!(
        read::<Multirange<DateTimeWithTimeZone>>(
            db,
            "SELECT $1::tstzmultirange",
            instants.clone().into()
        )
        .await?,
        instants
    );
    Ok(())
}

/// One value, rendered inline by `to_string` and bound by `build`, reads back
/// as one value: the constructor-call literal and the binary encoding agree
/// on every bound, on the empty range, and on a NULL bound, which both read
/// as no bound.
// [spec:pgorm:sem:sql.value.render+2/test]
// [spec:pgorm:req:exec.cursor.binding-range/test]
async fn the_bound_and_inline_renderings_agree(db: &DatabaseConnection) -> Result<(), Error> {
    async fn both<T>(db: &DatabaseConnection, value: Value, cast: &'static str) -> Result<T, Error>
    where
        T: TryGetable + PartialEq + std::fmt::Debug,
    {
        let query = Query::select()
            .expr(Expr::val(value).cast_as(alias(cast)))
            .to_owned();
        let inline: T = (query.to_string(), Values(Vec::new()))
            .into_tuple()
            .one(db)
            .await?;
        let (sql, values) = query.build();
        let bound: T = (sql, values).into_tuple().one(db).await?;
        assert_eq!(inline, bound, "{query}");
        Ok(bound)
    }

    let null_lower = Value::Range(
        pgorm::pgorm_query::RangeType::Int4,
        Some(Box::new(Range::new(
            Included(Value::Int(None)),
            Included(Value::Int(Some(5))),
        ))),
    );
    assert_eq!(
        both::<Range<i32>>(db, null_lower, "int4range").await?,
        Range::from(..6)
    );
    assert_eq!(
        both::<Range<i32>>(db, Range::<i32>::Empty.into(), "int4range").await?,
        Range::Empty
    );
    assert_eq!(
        both::<Range<i32>>(db, Range::new(Excluded(1), Included(5)).into(), "int4range").await?,
        Range::from(2..6)
    );
    let amounts = Range::new(Included(dec(-150, 2)), Unbounded);
    assert_eq!(
        both::<Range<Decimal>>(db, amounts.clone().into(), "numrange").await?,
        amounts
    );
    let stamps = Range::from(stamp(2024, 2, 29, 12)..stamp(2024, 3, 1, 0));
    assert_eq!(
        both::<Range<DateTime>>(db, stamps.clone().into(), "tsrange").await?,
        stamps
    );
    let instants = Range::new(Excluded(instant(1_700_000_000, 5)), Unbounded);
    assert_eq!(
        both::<Range<DateTimeWithTimeZone>>(db, instants.clone().into(), "tstzrange").await?,
        instants
    );
    let days: Multirange<Date> = [
        Range::from(date(2024, 1, 1)..date(2024, 1, 5)),
        Range::Empty,
    ]
    .into_iter()
    .collect();
    assert_eq!(
        both::<Multirange<Date>>(db, days.into(), "datemultirange").await?,
        [Range::from(date(2024, 1, 1)..date(2024, 1, 5))]
            .into_iter()
            .collect()
    );
    assert_eq!(
        both::<Multirange<i64>>(db, Multirange::<i64>::default().into(), "int8multirange").await?,
        Multirange::default()
    );

    // An array of ranges binds element by element and renders as an array of
    // constructor calls; there is no `Vec<Range<T>>` to read it back into, so
    // an element is read instead.
    let ranges = Value::array([Range::from(1i32..3), Range::Empty, Range::from(..=9)]);
    let bound: Range<i32> = read(db, "SELECT ($1::int4range[])[3]", ranges.clone()).await?;
    let inline: Range<i32> = (format!("SELECT ({ranges})[3]"), Values(Vec::new()))
        .into_tuple()
        .one(db)
        .await?;
    assert_eq!(bound, Range::from(..10));
    assert_eq!(inline, bound);
    let empty: bool = read(db, "SELECT isempty(($1::int4range[])[2])", ranges).await?;
    assert!(empty);
    Ok(())
}

/// The containment and overlap operators take a bound range; an element
/// beside a range is typed as the range by the server, so it is refused
/// before it is sent unless the caller pins it to the subtype.
// [spec:pgorm:req:exec.cursor.binding-range/test]
async fn the_operators_read_ranges(db: &DatabaseConnection) -> Result<(), Error> {
    let ids = |condition: SimpleExpr| {
        spans::Entity::find()
            .select([spans::Column::Id])
            .filter(condition)
            .order_by_asc(spans::Column::Id)
            .into_tuple::<i32>()
    };
    let counts = || Expr::col(spans::Column::Counts);
    assert_eq!(
        ids(counts().contains(Range::from(2..4))).all(db).await?,
        [1, 2]
    );
    assert_eq!(
        ids(counts().contains(Range::from(2..7))).all(db).await?,
        Vec::<i32>::new()
    );
    assert_eq!(
        ids(counts().contained(Range::from(0..10))).all(db).await?,
        [1, 2]
    );
    assert_eq!(
        ids(counts().binary(BinOper::Overlap, Range::from(5..9)))
            .all(db)
            .await?,
        [1, 2]
    );
    assert_eq!(
        ids(counts().binary(BinOper::Overlap, Range::from(6..9)))
            .all(db)
            .await?,
        Vec::<i32>::new()
    );

    let unpinned = ids(counts().contains(3)).all(db).await.unwrap_err();
    assert!(
        unpinned
            .to_string()
            .contains("cannot bind a `Int` value to Postgres type `int4range`"),
        "{unpinned}"
    );
    let pinned = counts().contains(Expr::val(3).cast_as(alias("int4")));
    assert_eq!(ids(pinned).all(db).await?, [1, 2]);
    Ok(())
}

/// A range type a schema creates binds and decodes through the same
/// `Range<T>` as the built-in over its subtype, because its wire format is
/// the subtype's; with no canonical function it stores bounds as written. Its
/// multirange reaches the driver as a simple type, so it is refused both
/// ways rather than guessed at.
// [spec:pgorm:req:exec.cursor.binding-range/test]
// [spec:pgorm:def:exec.decode.range/test]
// [spec:pgorm:req:sql.ddl.type-range/test]
async fn a_created_range_type_binds_by_its_subtype(db: &DatabaseConnection) -> Result<(), Error> {
    db.batch_execute(
        &Type::create(Name::runtime("slot"))
            .as_range(RangeDefinition::new(ColumnType::Integer))
            .to_string(),
    )
    .await?;
    db.batch_execute(
        &Table::create(Name::runtime("slots"))
            .col(ColumnDef::new(Name::runtime("id")).integer())
            .col(ColumnDef::new_with_type(
                Name::runtime("span"),
                ColumnType::named("slot"),
            ))
            .to_string(),
    )
    .await?;
    let span = Range::from(1i32..=5);
    let inserted = Query::insert()
        .into_table(Name::runtime("slots"))
        .columns([Name::runtime("id"), Name::runtime("span")])
        .values_panic([1.into(), span.clone().into()])
        .returning_col(Name::runtime("span"))
        .to_owned();
    let (sql, values) = inserted.build();
    let returned: Range<i32> = (sql, values).into_tuple().one(db).await?;
    assert_eq!(returned, span);

    let multirange = read::<Multirange<i32>>(
        db,
        "SELECT $1::slot_multirange",
        Multirange::<i32>::default().into(),
    )
    .await
    .unwrap_err();
    assert!(
        multirange
            .to_string()
            .contains("cannot bind a `Multirange` value to Postgres type `slot_multirange`"),
        "{multirange}"
    );
    let decoded = read::<Multirange<i32>>(
        db,
        "SELECT '{[1,2)}'::slot_multirange WHERE $1::int4 IS NOT NULL",
        1.into(),
    )
    .await;
    assert!(
        decoded.is_err(),
        "a created multirange decoded as {decoded:?}"
    );
    Ok(())
}

/// What the server refuses, it refuses with its own code: bounds out of
/// order, and a discrete bound its canonical form would overflow.
// [spec:pgorm:def:sql.value.range/test]
async fn the_server_refuses_what_it_cannot_store(db: &DatabaseConnection) -> Result<(), Error> {
    let inverted = read::<Range<i32>>(
        db,
        "SELECT $1::int4range",
        Range::new(Included(5), Excluded(1)).into(),
    )
    .await
    .unwrap_err();
    refused_with(&inverted, &SqlState::DATA_EXCEPTION);
    let overflow = read::<Range<i32>>(db, "SELECT $1::int4range", Range::from(1..=i32::MAX).into())
        .await
        .unwrap_err();
    refused_with(&overflow, &SqlState::NUMERIC_VALUE_OUT_OF_RANGE);
    Ok(())
}
