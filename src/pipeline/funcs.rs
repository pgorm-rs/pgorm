//! Literals, aggregate and window functions, `case`, and the cast type set.

use super::adapter;
use super::expr::{Expr, branded, out};

/// An integer literal.
///
/// Literals are for developer-authored constants; runtime values belong in
/// [`Binder::bind`](super::Binder::bind), which keeps them out of the SQL
/// text.
pub fn lit_int<'brand>(value: i64) -> Expr<'brand> {
    branded(adapter::lit_int(value))
}

/// A float literal.
pub fn lit_float<'brand>(value: f64) -> Expr<'brand> {
    branded(adapter::lit_float(value))
}

/// A string literal.
pub fn lit_str<'brand>(value: &str) -> Expr<'brand> {
    branded(adapter::lit_str(value))
}

/// A boolean literal.
pub fn lit_bool<'brand>(value: bool) -> Expr<'brand> {
    branded(adapter::lit_bool(value))
}

/// The `NULL` literal. Comparing against it with
/// [`eq`](Expr::eq) / [`ne`](Expr::ne) emits `IS [NOT] NULL`.
pub fn null<'brand>() -> Expr<'brand> {
    branded(adapter::lit_null())
}

fn unary_fn<'brand>(name: &str, expr: Expr<'brand>) -> Expr<'brand> {
    branded(adapter::call(name, vec![expr.node]))
}

/// `SUM(expr)`.
pub fn sum(expr: Expr<'_>) -> Expr<'_> {
    unary_fn("sum", expr)
}

/// `MIN(expr)`.
pub fn min(expr: Expr<'_>) -> Expr<'_> {
    unary_fn("min", expr)
}

/// `MAX(expr)`.
pub fn max(expr: Expr<'_>) -> Expr<'_> {
    unary_fn("max", expr)
}

/// `AVG(expr)`.
pub fn average(expr: Expr<'_>) -> Expr<'_> {
    unary_fn("average", expr)
}

/// `STDDEV(expr)`.
pub fn stddev(expr: Expr<'_>) -> Expr<'_> {
    unary_fn("stddev", expr)
}

/// `COUNT(expr)`.
pub fn count(expr: Expr<'_>) -> Expr<'_> {
    unary_fn("count", expr)
}

/// `COUNT(*)`.
pub fn count_rows<'brand>() -> Expr<'brand> {
    unary_fn("count", out("this"))
}

/// `COUNT(DISTINCT expr)`.
pub fn count_distinct(expr: Expr<'_>) -> Expr<'_> {
    unary_fn("count_distinct", expr)
}

/// `ROW_NUMBER()` over the enclosing window.
pub fn row_number<'brand>() -> Expr<'brand> {
    unary_fn("row_number", out("this"))
}

/// `RANK()` over the enclosing window.
///
/// PRQL's `rank` takes the ranked column (the rank itself comes from the
/// window's ordering; the argument names what is being ranked).
pub fn rank(expr: Expr<'_>) -> Expr<'_> {
    unary_fn("rank", expr)
}

/// `DENSE_RANK()` over the enclosing window; argument as in [`rank`].
pub fn rank_dense(expr: Expr<'_>) -> Expr<'_> {
    unary_fn("rank_dense", expr)
}

/// `LAG(expr, offset)` over the enclosing window.
pub fn lag(offset: i64, expr: Expr<'_>) -> Expr<'_> {
    branded(adapter::call(
        "lag",
        vec![adapter::lit_int(offset), expr.node],
    ))
}

/// `LEAD(expr, offset)` over the enclosing window.
pub fn lead(offset: i64, expr: Expr<'_>) -> Expr<'_> {
    branded(adapter::call(
        "lead",
        vec![adapter::lit_int(offset), expr.node],
    ))
}

/// `FIRST_VALUE(expr)` over the enclosing window.
pub fn first(expr: Expr<'_>) -> Expr<'_> {
    unary_fn("first", expr)
}

/// `LAST_VALUE(expr)` over the enclosing window.
pub fn last(expr: Expr<'_>) -> Expr<'_> {
    unary_fn("last", expr)
}

/// A `CASE WHEN ... THEN ... ELSE ... END` expression.
///
/// Each arm is a `(condition, value)` pair, tried in order; `otherwise` is
/// mandatory, so an unmatched row yields the stated fallback rather than an
/// implicit `NULL` (make the fallback [`null`] to say that explicitly).
pub fn case<'brand>(
    arms: Vec<(Expr<'brand>, Expr<'brand>)>,
    otherwise: Expr<'brand>,
) -> Expr<'brand> {
    let mut pairs: Vec<_> = arms
        .into_iter()
        .map(|(condition, value)| (condition.node, value.node))
        .collect();
    pairs.push((adapter::lit_bool(true), otherwise.node));
    branded(adapter::case(pairs))
}

/// The types [`Expr::cast`] can target.
///
/// A closed set: the name reaches the SQL text verbatim as `CAST(x AS name)`,
/// so an open string here would be an interpolation hole. Types outside this
/// list are outside the v1 surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CastType {
    /// `smallint`
    SmallInt,
    /// `integer`
    Integer,
    /// `bigint`
    BigInt,
    /// `real`
    Real,
    /// `float8` (double precision)
    Double,
    /// `numeric`
    Numeric,
    /// `text`
    Text,
    /// `boolean`
    Boolean,
    /// `date`
    Date,
    /// `timestamp`
    Timestamp,
    /// `timestamptz`
    Timestamptz,
    /// `interval`
    Interval,
    /// `uuid`
    Uuid,
    /// `json`
    Json,
    /// `jsonb`
    Jsonb,
}

impl CastType {
    pub(super) fn name(self) -> &'static str {
        match self {
            CastType::SmallInt => "smallint",
            CastType::Integer => "integer",
            CastType::BigInt => "bigint",
            CastType::Real => "real",
            CastType::Double => "float8",
            CastType::Numeric => "numeric",
            CastType::Text => "text",
            CastType::Boolean => "boolean",
            CastType::Date => "date",
            CastType::Timestamp => "timestamp",
            CastType::Timestamptz => "timestamptz",
            CastType::Interval => "interval",
            CastType::Uuid => "uuid",
            CastType::Json => "json",
            CastType::Jsonb => "jsonb",
        }
    }
}
