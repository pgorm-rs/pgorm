//! Membership predicates on [`Expr`]: `IN` lists, `= ANY`/`<> ALL` array
//! parameters, tuple membership, and subquery membership.

use super::*;

impl Expr {
    /// Express a `IN` expression.
    ///
    /// One placeholder per element, so each list length is a distinct
    /// statement: fine for a short literal list, and [`Expr::eq_any`] is the
    /// PostgreSQL-idiomatic alternative when the length varies at runtime.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .columns([Char::Id])
    ///     .from(Char::Table)
    ///     .and_where(Expr::col((Char::Table, Char::SizeW)).is_in([1, 2, 3]))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "id" FROM "character" WHERE "character"."size_w" IN (1, 2, 3)"#
    /// );
    /// ```
    /// Empty value list
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .columns([Char::Id])
    ///     .from(Char::Table)
    ///     .and_where(Expr::col((Char::Table, Char::SizeW)).is_in(Vec::<i32>::new()))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "id" FROM "character" WHERE 'a' = 'b'"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.expr.in+1]
    #[allow(clippy::wrong_self_convention)]
    pub fn is_in<V, I>(mut self, v: I) -> SimpleExpr
    where
        V: Into<SimpleExpr>,
        I: IntoIterator<Item = V>,
    {
        self.bopr = Some(BinOper::In);
        self.right = Some(SimpleExpr::Tuple(v.into_iter().map(|v| v.into()).collect()));
        self.into()
    }

    /// Express a `= ANY` membership test against one array parameter.
    ///
    /// The PostgreSQL-idiomatic counterpart of [`Expr::is_in`]: where `IN`
    /// spends one placeholder per element — so every list length is a distinct
    /// statement, and a long list walks toward the 65535-parameter ceiling —
    /// `= ANY` spends exactly one, whatever the list length. The SQL text is
    /// then the same for two elements and for two thousand, which is what a
    /// per-connection prepared-statement cache keys on.
    ///
    /// Reach for `is_in` when the list is a short literal written into the
    /// query, and for `eq_any` when its length varies at runtime.
    ///
    /// An empty list needs no special case: `= ANY` over an empty array is
    /// false for every operand, NULL included, which is the vacuous truth
    /// `is_in` has to fall back to a constant comparison to express.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .columns([Char::Id])
    ///     .from(Char::Table)
    ///     .and_where(Expr::col((Char::Table, Char::SizeW)).eq_any([1, 2, 3]))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "id" FROM "character" WHERE "character"."size_w" = ANY(ARRAY [1,2,3])"#
    /// );
    ///
    /// let (sql, values) = query.build();
    /// assert_eq!(
    ///     sql,
    ///     r#"SELECT "id" FROM "character" WHERE "character"."size_w" = ANY($1)"#
    /// );
    /// assert_eq!(values.0.len(), 1);
    /// ```
    /// Empty value list
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .columns([Char::Id])
    ///     .from(Char::Table)
    ///     .and_where(Expr::col((Char::Table, Char::SizeW)).eq_any(Vec::<i32>::new()))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "id" FROM "character" WHERE "character"."size_w" = ANY(ARRAY []::int4[])"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.expr.eq-any]
    pub fn eq_any<V, I>(self, v: I) -> SimpleExpr
    where
        V: Into<Value> + ValueType,
        I: IntoIterator<Item = V>,
    {
        self.bin_op(BinOper::Equal, Func::any(Value::array(v)))
    }

    /// Express a `<> ALL` non-membership test against one array parameter.
    ///
    /// The negation of [`Expr::eq_any`], spelled the way PostgreSQL itself
    /// spells it: its parser reads `x NOT IN (…)` as the `<>` operator over the
    /// list, never as a `NOT` wrapped around `x = ANY(…)`. The two are
    /// equivalent under three-valued logic — a NULL element makes both NULL,
    /// exactly as `NOT IN` does — so the choice is which shape to hand the
    /// planner, and `<> ALL` is one operator node where the wrapped form is a
    /// boolean node around a second one.
    ///
    /// An empty list is true for every operand, NULL included: nothing fails a
    /// test applied to nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .columns([Char::Id])
    ///     .from(Char::Table)
    ///     .and_where(Expr::col((Char::Table, Char::SizeW)).ne_all([1, 2, 3]))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "id" FROM "character" WHERE "character"."size_w" <> ALL(ARRAY [1,2,3])"#
    /// );
    ///
    /// let (sql, values) = query.build();
    /// assert_eq!(
    ///     sql,
    ///     r#"SELECT "id" FROM "character" WHERE "character"."size_w" <> ALL($1)"#
    /// );
    /// assert_eq!(values.0.len(), 1);
    /// ```
    // [spec:pgorm:req:sql.ast.expr.eq-any]
    pub fn ne_all<V, I>(self, v: I) -> SimpleExpr
    where
        V: Into<Value> + ValueType,
        I: IntoIterator<Item = V>,
    {
        self.bin_op(BinOper::NotEqual, Func::all(Value::array(v)))
    }

    /// Express a `IN` sub expression.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .columns([Char::Character, Char::FontId])
    ///     .from(Char::Table)
    ///     .and_where(
    ///         Expr::tuple([
    ///             Expr::col(Char::Character).into(),
    ///             Expr::col(Char::FontId).into(),
    ///         ])
    ///         .in_tuples([(1, String::from("1")), (2, String::from("2"))])
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "character", "font_id" FROM "character" WHERE ("character", "font_id") IN ((1, '1'), (2, '2'))"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.expr.in+1]
    #[allow(clippy::wrong_self_convention)]
    pub fn in_tuples<V, I>(mut self, v: I) -> SimpleExpr
    where
        V: IntoValueTuple,
        I: IntoIterator<Item = V>,
    {
        self.bopr = Some(BinOper::In);
        self.right = Some(SimpleExpr::Tuple(
            v.into_iter()
                .map(|m| SimpleExpr::Values(m.into_value_tuple().into_iter().collect()))
                .collect(),
        ));
        self.into()
    }

    /// Express a `NOT IN` expression. See [`Expr::ne_all`] for the
    /// array-parameter alternative.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .columns([Char::Id])
    ///     .from(Char::Table)
    ///     .and_where(Expr::col((Char::Table, Char::SizeW)).is_not_in([1, 2, 3]))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "id" FROM "character" WHERE "character"."size_w" NOT IN (1, 2, 3)"#
    /// );
    /// ```
    /// Empty value list
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .columns([Char::Id])
    ///     .from(Char::Table)
    ///     .and_where(Expr::col((Char::Table, Char::SizeW)).is_not_in(Vec::<i32>::new()))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "id" FROM "character" WHERE 'a' = 'a'"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.expr.in+1]
    #[allow(clippy::wrong_self_convention)]
    pub fn is_not_in<V, I>(mut self, v: I) -> SimpleExpr
    where
        V: Into<SimpleExpr>,
        I: IntoIterator<Item = V>,
    {
        self.bopr = Some(BinOper::NotIn);
        self.right = Some(SimpleExpr::Tuple(v.into_iter().map(|v| v.into()).collect()));
        self.into()
    }

    /// Express a `IN` sub-query expression.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .columns([Char::Character, Char::SizeW, Char::SizeH])
    ///     .from(Char::Table)
    ///     .and_where(Expr::col(Char::SizeW).in_subquery(
    ///         Query::select()
    ///             .expr(Expr::cust("3 + 2 * 2"))
    ///             .take()
    ///     ))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "character", "size_w", "size_h" FROM "character" WHERE "size_w" IN (SELECT 3 + 2 * 2)"#
    /// );
    /// ```
    #[allow(clippy::wrong_self_convention)]
    pub fn in_subquery(mut self, sel: SelectStatement) -> SimpleExpr {
        self.bopr = Some(BinOper::In);
        self.right = Some(SimpleExpr::SubQuery(
            None,
            Box::new(sel.into_sub_query_statement()),
        ));
        self.into()
    }

    /// Express a `NOT IN` sub-query expression.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .columns([Char::Character, Char::SizeW, Char::SizeH])
    ///     .from(Char::Table)
    ///     .and_where(Expr::col(Char::SizeW).not_in_subquery(
    ///         Query::select()
    ///             .expr(Expr::cust("3 + 2 * 2"))
    ///             .take()
    ///     ))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "character", "size_w", "size_h" FROM "character" WHERE "size_w" NOT IN (SELECT 3 + 2 * 2)"#
    /// );
    /// ```
    #[allow(clippy::wrong_self_convention)]
    pub fn not_in_subquery(mut self, sel: SelectStatement) -> SimpleExpr {
        self.bopr = Some(BinOper::NotIn);
        self.right = Some(SimpleExpr::SubQuery(
            None,
            Box::new(sel.into_sub_query_statement()),
        ));
        self.into()
    }
}
