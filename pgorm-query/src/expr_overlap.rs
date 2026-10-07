//! Overlap (`&&`): whether two ranges, multiranges or arrays share a value.
//!
//! It is the third of the containment family beside [`Expr::contains`] (`@>`)
//! and [`Expr::contained`] (`<@`), in a child module of `expr` rather than in
//! [`Expr`]'s main block, as the JSON and membership families are.

use super::*;

impl Expr {
    /// Express a postgres overlap (`&&`) expression: whether the two operands
    /// have a value in common.
    ///
    /// One operator serves ranges, multiranges (against a range or another
    /// multirange) and arrays, so this is each of their overlap tests. A bound
    /// right operand beside a range column is typed as that range by the
    /// server, as it is for [`contains`](Expr::contains): an element is not an
    /// operand of `&&` at all (`42883`), and the empty range overlaps nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .column(Char::Id)
    ///     .from(Char::Table)
    ///     .and_where(Expr::col(Char::Character).overlaps(Range::from(1..5)))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "id" FROM "character" WHERE "character" && int4range(1, 5, '[)')"#
    /// );
    /// assert_eq!(
    ///     query.build().0,
    ///     r#"SELECT "id" FROM "character" WHERE "character" && $1"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.expr.operators+4]
    pub fn overlaps<T>(self, expr: T) -> SimpleExpr
    where
        T: Into<SimpleExpr>,
    {
        self.bin_op(BinOper::Overlap, expr)
    }
}
