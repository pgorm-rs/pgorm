//! `COLLATE`: an expression under a named collation.
//!
//! It sits in a child module of `expr`, as the subscript family does, and like
//! that family it returns an [`Expr`] rather than a finished [`SimpleExpr`]: a
//! collated value is an operand, not a predicate, so it goes on to be
//! compared or ordered in the same chain.

use super::*;

impl Expr {
    /// Put this expression under the named collation: `(expr COLLATE "name")`.
    ///
    /// A collation decides how text compares and sorts, so the clause changes
    /// what `=`, `<` and `ORDER BY` answer about the same values: under `"C"`
    /// text orders by byte, every upper-case letter before every lower-case
    /// one, where a linguistic collation interleaves them. The right-hand side
    /// is a collation's *name*, quoted like every other identifier — see
    /// [`Collation`] for the qualified form — and never an expression, so
    /// `a COLLATE b` for an arbitrary `b` has no construction.
    ///
    /// `ORDER BY` has no collation slot of its own: PostgreSQL's grammar reads
    /// the clause as part of the sort key's expression, so an ordering takes
    /// the collated expression through
    /// [`order_by_expr`](crate::OrderedStatement::order_by_expr).
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .column(Glyph::Image)
    ///     .from(Glyph::Table)
    ///     .and_where(Expr::col(Glyph::Image).collate(Name::runtime("C")).lt("b"))
    ///     .order_by_expr(
    ///         Expr::col(Glyph::Image).collate(Name::runtime("C")).into(),
    ///         Order::Desc,
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     [
    ///         r#"SELECT "image" FROM "glyph""#,
    ///         r#"WHERE ("image" COLLATE "C") < 'b'"#,
    ///         r#"ORDER BY ("image" COLLATE "C") DESC"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.expr.collate]
    pub fn collate<C>(self, collation: C) -> Expr
    where
        C: IntoCollation,
    {
        Expr::expr(SimpleExpr::Collate(
            Box::new(self.into()),
            Box::new(collation.into_collation()),
        ))
    }
}
