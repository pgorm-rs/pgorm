//! Array subscripts and slices: `a[i]`, `a[l:u]`, and the slices open at
//! either end, `a[l:]` and `a[:u]`.
//!
//! They sit in a child module of `expr` rather than in [`Expr`]'s main block,
//! as the JSON, membership and keyword-operator families do. Unlike those,
//! each returns an [`Expr`] rather than a finished [`SimpleExpr`]: a
//! subscripted value is an operand, not a predicate, so it goes on to be
//! compared (`.index(1).gt(5)`) or subscripted again (`.index(1).index(2)`)
//! in the same chain.

use super::*;

/// What goes between the brackets of an array subscript.
///
/// PostgreSQL arrays count from 1. An [`Index`](Self::Index) selects one
/// element and answers NULL when it is out of range; a
/// [`Slice`](Self::Slice) selects the elements between its bounds, both
/// inclusive, and answers an array — cut to the array's bounds, and empty
/// rather than NULL when nothing overlaps. An omitted slice bound runs to
/// that end of the array, so `Slice(None, None)` is `[:]`, the whole of that
/// dimension.
// [spec:pgorm:req:sql.ast.expr.subscript]
#[derive(Debug, Clone, PartialEq)]
pub enum Subscript {
    /// `[index]`
    Index(SimpleExpr),
    /// `[lower:upper]`, either bound omissible
    Slice(Option<SimpleExpr>, Option<SimpleExpr>),
}

impl Expr {
    /// Subscript one element: `a[index]`.
    ///
    /// PostgreSQL arrays count from 1, and an index outside the array's
    /// bounds yields NULL rather than an error. Chained indexes are one
    /// multi-dimensional access — `.index(1).index(2)` is `a[1][2]`, the
    /// element at row 1, column 2 of a two-dimensional array. A bound index
    /// is a placeholder the server types as `int4`.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .expr(Expr::col(Glyph::Tokens).index(1))
    ///     .from(Glyph::Table)
    ///     .and_where(Expr::col(Glyph::Tokens).index(2).gt(0))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "tokens"[1] FROM "glyph" WHERE "tokens"[2] > 0"#
    /// );
    /// ```
    pub fn index<T>(self, index: T) -> Expr
    where
        T: Into<SimpleExpr>,
    {
        self.subscript(Subscript::Index(index.into()))
    }

    /// Subscript a slice between two bounds, both inclusive: `a[lower:upper]`.
    ///
    /// A slice is an array, and a slice reaching past the array's bounds is
    /// cut to them — an empty array when none of it overlaps — rather than
    /// NULL. PostgreSQL treats *every* subscript in a chain as a slice once
    /// any of them is one, reading a plain index `i` beside a slice as `1:i`.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .expr(Expr::col(Glyph::Tokens).slice(2, 3))
    ///     .from(Glyph::Table)
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "tokens"[2:3] FROM "glyph""#
    /// );
    /// ```
    pub fn slice<L, U>(self, lower: L, upper: U) -> Expr
    where
        L: Into<SimpleExpr>,
        U: Into<SimpleExpr>,
    {
        self.subscript(Subscript::Slice(Some(lower.into()), Some(upper.into())))
    }

    /// Subscript a slice from `lower` to the array's end: `a[lower:]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .expr(Expr::col(Glyph::Tokens).slice_from(2))
    ///     .from(Glyph::Table)
    ///     .to_owned();
    ///
    /// assert_eq!(query.to_string(), r#"SELECT "tokens"[2:] FROM "glyph""#);
    /// ```
    pub fn slice_from<L>(self, lower: L) -> Expr
    where
        L: Into<SimpleExpr>,
    {
        self.subscript(Subscript::Slice(Some(lower.into()), None))
    }

    /// Subscript a slice from the array's start to `upper`: `a[:upper]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .expr(Expr::col(Glyph::Tokens).slice_to(2))
    ///     .from(Glyph::Table)
    ///     .to_owned();
    ///
    /// assert_eq!(query.to_string(), r#"SELECT "tokens"[:2] FROM "glyph""#);
    /// ```
    pub fn slice_to<U>(self, upper: U) -> Expr
    where
        U: Into<SimpleExpr>,
    {
        self.subscript(Subscript::Slice(None, Some(upper.into())))
    }

    /// Apply any [`Subscript`], including the slice with neither bound,
    /// `a[:]`, which the four shorthands do not spell.
    ///
    /// A column reference or an already-subscripted expression takes the
    /// subscript directly; anything else — a function call, a cast, a bound
    /// value — is parenthesised first, because PostgreSQL's grammar admits a
    /// subscript only after a column, a parameter or a parenthesised
    /// expression.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .expr(Expr::col(Glyph::Tokens).subscript(Subscript::Slice(None, None)))
    ///     .expr(Expr::expr(Func::named(Name::runtime("string_to_array"))
    ///         .arg(Expr::col(Glyph::Image))
    ///         .arg(",")).index(1))
    ///     .from(Glyph::Table)
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "tokens"[:], (string_to_array("image", ','))[1] FROM "glyph""#
    /// );
    /// ```
    pub fn subscript(self, subscript: Subscript) -> Expr {
        Expr::expr(SimpleExpr::Subscript(
            Box::new(self.into()),
            Box::new(subscript),
        ))
    }
}
