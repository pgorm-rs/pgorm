use crate::{
    AnyWithClause, QueryStatementBuilder, ReturningClause, SimpleExpr, SubQueryStatement,
    WithQuery, backend::QueryBuilder, prepare::*, query::condition::*, types::*, value::*,
};
use inherent::inherent;

/// Delete existing rows from the table
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let query = Query::delete()
///     .from_table(Glyph::Table)
///     .cond_where(any![
///         Expr::col(Glyph::Id).lt(1),
///         Expr::col(Glyph::Id).gt(10),
///     ])
///     .to_owned();
///
/// assert_eq!(
///     query.to_string(),
///     r#"DELETE FROM "glyph" WHERE "id" < 1 OR "id" > 10"#
/// );
/// ```
///
/// PostgreSQL admits neither `ORDER BY` nor `LIMIT` on a DELETE, so the
/// statement carries neither and an ordered delete does not typecheck:
///
/// ```compile_fail,E0599
/// use pgorm_query::{tests_cfg::*, *};
///
/// Query::delete()
///     .from_table(Glyph::Table)
///     .order_by(Glyph::Id, Order::Asc);
/// ```
///
/// nor a limited one:
///
/// ```compile_fail,E0599
/// use pgorm_query::{tests_cfg::*, *};
///
/// Query::delete().from_table(Glyph::Table).limit(1);
/// ```
///
/// Both belong to the SELECT that chooses the rows, so a delete over an
/// ordered, limited set is spelled as a subquery filter:
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let query = Query::delete()
///     .from_table(Glyph::Table)
///     .and_where(Expr::col(Glyph::Id).in_subquery(
///         Query::select()
///             .column(Glyph::Id)
///             .from(Glyph::Table)
///             .order_by(Glyph::Id, Order::Asc)
///             .limit(1)
///             .take(),
///     ))
///     .to_owned();
///
/// assert_eq!(
///     query.to_string(),
///     r#"DELETE FROM "glyph" WHERE "id" IN (SELECT "id" FROM "glyph" ORDER BY "id" ASC LIMIT 1)"#
/// );
/// ```
// [spec:pgorm:def:sql.ast.delete+2]
#[derive(Default, Debug, Clone, PartialEq)]
pub struct DeleteStatement {
    pub(crate) table: Option<NamedTable>,
    pub(crate) r#where: ConditionHolder,
    pub(crate) returning: Option<ReturningClause>,
}

impl DeleteStatement {
    /// Construct a new [`DeleteStatement`]
    pub fn new() -> Self {
        Self::default()
    }

    /// Specify which table to delete from.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::delete()
    ///     .from_table(Glyph::Table)
    ///     .and_where(Expr::col(Glyph::Id).eq(1))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"DELETE FROM "glyph" WHERE "id" = 1"#
    /// );
    /// ```
    ///
    /// The target is a name, optionally aliased:
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::delete()
    ///     .from_table(Glyph::Table.into_named_table().alias(Alias::new("g")))
    ///     .and_where(Expr::col((Alias::new("g"), Glyph::Id)).eq(1))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"DELETE FROM "glyph" AS "g" WHERE "g"."id" = 1"#
    /// );
    /// ```
    // [spec:pgorm:def:sql.ast.delete+2]
    #[allow(clippy::wrong_self_convention)]
    pub fn from_table<T>(&mut self, tbl_ref: T) -> &mut Self
    where
        T: IntoNamedTable,
    {
        self.table = Some(tbl_ref.into_named_table());
        self
    }

    /// RETURNING expressions.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::delete()
    ///     .from_table(Glyph::Table)
    ///     .and_where(Expr::col(Glyph::Id).eq(1))
    ///     .returning(Query::returning().columns([Glyph::Id]))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"DELETE FROM "glyph" WHERE "id" = 1 RETURNING "id""#
    /// );
    /// ```
    pub fn returning(&mut self, returning_cols: ReturningClause) -> &mut Self {
        self.returning = Some(returning_cols);
        self
    }

    /// RETURNING expressions for a column.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::delete()
    ///     .from_table(Glyph::Table)
    ///     .and_where(Expr::col(Glyph::Id).eq(1))
    ///     .returning_col(Glyph::Id)
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"DELETE FROM "glyph" WHERE "id" = 1 RETURNING "id""#
    /// );
    /// ```
    pub fn returning_col<C>(&mut self, col: C) -> &mut Self
    where
        C: IntoColumnRef,
    {
        self.returning(ReturningClause::Columns(vec![col.into_column_ref()]))
    }

    /// RETURNING expressions all columns.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::delete()
    ///     .from_table(Glyph::Table)
    ///     .and_where(Expr::col(Glyph::Id).eq(1))
    ///     .returning_all()
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"DELETE FROM "glyph" WHERE "id" = 1 RETURNING *"#
    /// );
    /// ```
    pub fn returning_all(&mut self) -> &mut Self {
        self.returning(ReturningClause::All)
    }

    /// Create a [WithQuery] by specifying a with clause to execute this query with. The clause is
    /// either a [`WithClause`](crate::WithClause) or a
    /// [`RecursiveWithClause`](crate::RecursiveWithClause).
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, IntoCondition, IntoIden, tests_cfg::*};
    ///
    /// let select = SelectStatement::new()
    ///         .columns([Glyph::Id])
    ///         .from(Glyph::Table)
    ///         .and_where(Expr::col(Glyph::Image).like("0%"))
    ///         .to_owned();
    ///     let cte = CommonTableExpression::new(Alias::new("cte"), select)
    ///         .column(Glyph::Id)
    ///         .to_owned();
    ///     let with_clause = WithClause::new(cte);
    ///     let update = DeleteStatement::new()
    ///         .from_table(Glyph::Table)
    ///         .and_where(Expr::col(Glyph::Id).in_subquery(SelectStatement::new().column(Glyph::Id).from(Alias::new("cte")).to_owned()))
    ///         .to_owned();
    ///     let query = update.with(with_clause);
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"WITH "cte" ("id") AS (SELECT "id" FROM "glyph" WHERE "image" LIKE '0%') DELETE FROM "glyph" WHERE "id" IN (SELECT "id" FROM "cte")"#
    /// );
    /// ```
    pub fn with<C>(self, clause: C) -> WithQuery
    where
        C: Into<AnyWithClause>,
    {
        WithQuery::new(clause, self)
    }
}

#[inherent]
impl QueryStatementBuilder for DeleteStatement {
    pub fn build_collect_into(&self, sql: &mut dyn SqlWriter) {
        QueryBuilder.prepare_delete_statement(self, sql);
    }

    pub fn into_sub_query_statement(self) -> SubQueryStatement {
        SubQueryStatement::DeleteStatement(self)
    }

    pub fn build(&self) -> (String, Values);
    pub fn build_collect(&self, sql: &mut dyn SqlWriter) -> String;
}

// [spec:pgorm:req:sql.ast.build+1] (the one value-inlined rendering)
impl std::fmt::Display for DeleteStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sql = String::with_capacity(256);
        QueryBuilder.prepare_delete_statement(self, &mut sql);
        f.write_str(&sql)
    }
}

#[inherent]
impl ConditionalStatement for DeleteStatement {
    pub fn cond_where<C>(&mut self, condition: C) -> &mut Self
    where
        C: IntoCondition,
    {
        self.r#where.add_condition(condition.into_condition());
        self
    }

    pub fn and_where_option(&mut self, other: Option<SimpleExpr>) -> &mut Self;
    pub fn and_where(&mut self, other: SimpleExpr) -> &mut Self;
}
