//! The infix operators PostgreSQL spells as a phrase rather than as a symbol:
//! null-safe comparison (`IS [NOT] DISTINCT FROM`), the bound-sorting range
//! test (`BETWEEN [NOT] SYMMETRIC`), time-zone conversion (`AT TIME ZONE`),
//! and the regular-expression match `[NOT] SIMILAR TO`.
//!
//! They sit in a child module of `expr` rather than in [`Expr`]'s main block,
//! as the JSON and membership families do. The first three each exist for an
//! edge its symbolic neighbour gets wrong: `<>` answers NULL where
//! `IS DISTINCT FROM` answers, and `BETWEEN` silently matches nothing where
//! `BETWEEN SYMMETRIC` sorts the pair. All but `AT TIME ZONE` depend on
//! [`crate::BinOper`] family membership to render correctly — see
//! `[spec:pgorm:def:sql.types.opers+5]`.

use super::*;

impl Expr {
    /// Express a `BETWEEN SYMMETRIC` expression, which sorts its two bounds
    /// before testing and so holds whichever order they are given in. Plain
    /// `BETWEEN` matches nothing when the larger bound is written first.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .columns([Char::Character, Char::SizeW, Char::SizeH])
    ///     .from(Char::Table)
    ///     .and_where(Expr::col((Char::Table, Char::SizeW)).between_symmetric(10, 1))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "character", "size_w", "size_h" FROM "character" WHERE "character"."size_w" BETWEEN SYMMETRIC 10 AND 1"#
    /// );
    /// ```
    pub fn between_symmetric<V>(self, a: V, b: V) -> SimpleExpr
    where
        V: Into<SimpleExpr>,
    {
        self.between_bounds(BinOper::BetweenSymmetric, a, b)
    }

    /// Express a `NOT BETWEEN SYMMETRIC` expression, the complement of
    /// [`Expr::between_symmetric`].
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .columns([Char::Character, Char::SizeW, Char::SizeH])
    ///     .from(Char::Table)
    ///     .and_where(Expr::col((Char::Table, Char::SizeW)).not_between_symmetric(10, 1))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "character", "size_w", "size_h" FROM "character" WHERE "character"."size_w" NOT BETWEEN SYMMETRIC 10 AND 1"#
    /// );
    /// ```
    pub fn not_between_symmetric<V>(self, a: V, b: V) -> SimpleExpr
    where
        V: Into<SimpleExpr>,
    {
        self.between_bounds(BinOper::NotBetweenSymmetric, a, b)
    }

    /// Express an `IS DISTINCT FROM` expression: inequality that reads NULL as
    /// a value rather than as unknown.
    ///
    /// `<>` is NULL whenever either side is, so a row with a NULL passes
    /// neither `a <> b` nor its negation. This operator answers true or false
    /// for every pair — two NULLs are not distinct, a NULL and a non-NULL are
    /// — which is what makes it the comparison to reach for when the column
    /// is nullable and the query means "changed".
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .columns([Char::Character, Char::SizeW, Char::SizeH])
    ///     .from(Char::Table)
    ///     .and_where(Expr::col((Char::Table, Char::SizeW)).is_distinct_from(1))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "character", "size_w", "size_h" FROM "character" WHERE "character"."size_w" IS DISTINCT FROM 1"#
    /// );
    /// ```
    #[allow(clippy::wrong_self_convention)]
    pub fn is_distinct_from<V>(self, v: V) -> SimpleExpr
    where
        V: Into<SimpleExpr>,
    {
        self.binary(BinOper::IsDistinctFrom, v)
    }

    /// Express an `IS NOT DISTINCT FROM` expression: the null-safe equality
    /// complementing [`Expr::is_distinct_from`].
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .columns([Char::Character, Char::SizeW, Char::SizeH])
    ///     .from(Char::Table)
    ///     .and_where(Expr::col((Char::Table, Char::SizeW)).is_not_distinct_from(1))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "character", "size_w", "size_h" FROM "character" WHERE "character"."size_w" IS NOT DISTINCT FROM 1"#
    /// );
    /// ```
    #[allow(clippy::wrong_self_convention)]
    pub fn is_not_distinct_from<V>(self, v: V) -> SimpleExpr
    where
        V: Into<SimpleExpr>,
    {
        self.binary(BinOper::IsNotDistinctFrom, v)
    }

    /// Express an `AT TIME ZONE` expression, reinterpreting a timestamp in the
    /// named zone.
    ///
    /// The direction follows the left operand's type, as PostgreSQL's operator
    /// does: applied to a `timestamptz` it yields the local `timestamp` in that
    /// zone, and applied to a `timestamp` it reads the value *as* that zone's
    /// local time and yields a `timestamptz`.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .expr(Expr::col(Char::CreatedAt).at_time_zone("UTC"))
    ///     .from(Char::Table)
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "created_at" AT TIME ZONE 'UTC' FROM "character""#
    /// );
    /// ```
    pub fn at_time_zone<V>(self, zone: V) -> SimpleExpr
    where
        V: Into<SimpleExpr>,
    {
        self.binary(BinOper::AtTimeZone, zone)
    }

    /// Express a `SIMILAR TO` expression: whether the whole string matches a
    /// pattern in SQL's regular-expression dialect, which is LIKE's `%` and
    /// `_` with `|`, `*`, `+`, `?`, `{m,n}`, grouping and bracket classes
    /// added. The pattern is bound like [`like`](Self::like)'s, and a
    /// [`LikeExpr`] carries an `ESCAPE` character the same way; the default
    /// escape is a backslash, and an escape longer than one character is
    /// PostgreSQL's refusal (`22025`).
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let (sql, values) = Query::select()
    ///     .column(Char::Character)
    ///     .from(Char::Table)
    ///     .and_where(Expr::col(Char::Character).similar_to("(A|B)%"))
    ///     .and_where(Expr::col(Char::Character).not_similar_to(LikeExpr::new("!%%").escape('!')))
    ///     .build();
    ///
    /// assert_eq!(
    ///     sql,
    ///     r#"SELECT "character" FROM "character" WHERE "character" SIMILAR TO $1 AND "character" NOT SIMILAR TO $2 ESCAPE '!'"#
    /// );
    /// assert_eq!(values.0, vec!["(A|B)%".into(), "!%%".into()]);
    /// ```
    // [spec:pgorm:req:sql.ast.expr.similar-to]
    pub fn similar_to<L>(self, pattern: L) -> SimpleExpr
    where
        L: IntoLikeExpr,
    {
        self.like_like(BinOper::SimilarTo, pattern.into_like_expr())
    }

    /// Express a `NOT SIMILAR TO` expression, the complement of
    /// [`Expr::similar_to`].
    // [spec:pgorm:req:sql.ast.expr.similar-to]
    pub fn not_similar_to<L>(self, pattern: L) -> SimpleExpr
    where
        L: IntoLikeExpr,
    {
        self.like_like(BinOper::NotSimilarTo, pattern.into_like_expr())
    }
}

impl SimpleExpr {
    /// Express a `SIMILAR TO` expression, as [`Expr::similar_to`] does.
    // [spec:pgorm:req:sql.ast.expr.similar-to]
    pub fn similar_to<L>(self, pattern: L) -> Self
    where
        L: IntoLikeExpr,
    {
        self.like_like(BinOper::SimilarTo, pattern.into_like_expr())
    }

    /// Express a `NOT SIMILAR TO` expression, as [`Expr::not_similar_to`]
    /// does.
    // [spec:pgorm:req:sql.ast.expr.similar-to]
    pub fn not_similar_to<L>(self, pattern: L) -> Self
    where
        L: IntoLikeExpr,
    {
        self.like_like(BinOper::NotSimilarTo, pattern.into_like_expr())
    }
}
