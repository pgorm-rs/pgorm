//! Attaching a WITH clause to a [`SelectStatement`]: the carried-clause
//! surface, in all three spellings.

use super::*;

impl SelectStatement {
    /// Attach a WITH clause — either a [`WithClause`](crate::WithClause) or a
    /// [`RecursiveWithClause`](crate::RecursiveWithClause) — to this select.
    ///
    /// The clause is carried *on* the statement rather than wrapping it, so the
    /// value stays a [`SelectStatement`]: `and_where`, `order_by`, `limit` and
    /// every other builder method still apply afterwards, and the statement
    /// still nests as a subquery, a union arm, a CTE body or a LATERAL body.
    /// The clause renders as a prefix at whatever level the statement occupies.
    ///
    /// The last call wins; a select carries at most one clause.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, IntoCondition, IntoIden, tests_cfg::*};
    ///
    /// let base_query = SelectStatement::new()
    ///                     .column(Alias::new("id"))
    ///                     .expr(1i32)
    ///                     .column(Alias::new("next"))
    ///                     .column(Alias::new("value"))
    ///                     .from(Alias::new("table"))
    ///                     .to_owned();
    ///
    /// let cte_referencing = SelectStatement::new()
    ///                             .column(Alias::new("id"))
    ///                             .expr(Expr::col(Alias::new("depth")).add(1i32))
    ///                             .column(Alias::new("next"))
    ///                             .column(Alias::new("value"))
    ///                             .from(Alias::new("table"))
    ///                             .join(
    ///                                 JoinType::InnerJoin,
    ///                                 Alias::new("cte_traversal"),
    ///                                 Expr::col((Alias::new("cte_traversal"), Alias::new("next"))).equals((Alias::new("table"), Alias::new("id")))
    ///                             )
    ///                             .to_owned();
    ///
    /// let common_table_expression = CommonTableExpression::new(
    ///             Alias::new("cte_traversal"),
    ///             base_query.clone().union(UnionType::All, cte_referencing).to_owned(),
    ///         )
    ///         .columns([Alias::new("id"), Alias::new("depth"), Alias::new("next"), Alias::new("value")])
    ///         .to_owned();
    ///
    /// let select = SelectStatement::new()
    ///         .column(ColumnRef::Asterisk)
    ///         .from(Alias::new("cte_traversal"))
    ///         .to_owned();
    ///
    /// let query = select.with(RecursiveWithClause::new(common_table_expression));
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"WITH RECURSIVE "cte_traversal" ("id", "depth", "next", "value") AS (SELECT "id", 1, "next", "value" FROM "table" UNION ALL (SELECT "id", "depth" + 1, "next", "value" FROM "table" INNER JOIN "cte_traversal" ON "cte_traversal"."next" = "table"."id")) SELECT * FROM "cte_traversal""#
    /// );
    /// ```
    // [spec:pgorm:def:query.build.with]
    // [spec:pgorm:sem:query.build.with.attach]
    pub fn with<C>(mut self, clause: C) -> Self
    where
        C: Into<AnyWithClause>,
    {
        self.with = Some(Box::new(clause.into()));
        self
    }

    /// Attach a non-recursive WITH clause in place, for the `&mut self` builder
    /// style. See [`with`](SelectStatement::with) for the semantics.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let cte = CommonTableExpression::new(
    ///     Alias::new("recent"),
    ///     Query::select().column(Glyph::Id).from(Glyph::Table).to_owned(),
    /// );
    ///
    /// let query = Query::select()
    ///     .column(Glyph::Id)
    ///     .from(Alias::new("recent"))
    ///     .with_cte(WithClause::new(cte))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"WITH "recent" AS (SELECT "id" FROM "glyph") SELECT "id" FROM "recent""#
    /// );
    /// ```
    // [spec:pgorm:def:query.build.with]
    // [spec:pgorm:sem:query.build.with.attach]
    pub fn with_cte(&mut self, clause: WithClause) -> &mut Self {
        self.with = Some(Box::new(clause.into()));
        self
    }

    /// Attach a `WITH RECURSIVE` clause in place, for the `&mut self` builder
    /// style. See [`with`](SelectStatement::with) for the semantics.
    // [spec:pgorm:def:query.build.with]
    // [spec:pgorm:sem:query.build.with.attach]
    pub fn with_recursive_cte(&mut self, clause: RecursiveWithClause) -> &mut Self {
        self.with = Some(Box::new(clause.into()));
        self
    }
}
