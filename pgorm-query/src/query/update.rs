use crate::{
    AnyWithClause, QueryStatementBuilder, ReturningClause, SubQueryStatement, WithQuery,
    backend::QueryBuilder, expr::*, prepare::*, query::condition::*, types::*, value::*,
};
use inherent::inherent;

/// Update existing rows in the table
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let query = Query::update()
///     .table(Glyph::Table)
///     .values([(Glyph::Aspect, 1.23.into()), (Glyph::Image, "123".into())])
///     .and_where(Expr::col(Glyph::Id).eq(1))
///     .to_owned();
///
/// assert_eq!(
///     query.to_string(),
///     r#"UPDATE "glyph" SET "aspect" = 1.23, "image" = '123' WHERE "id" = 1"#
/// );
/// ```
///
/// PostgreSQL admits neither `ORDER BY` nor `LIMIT` on an UPDATE, so the
/// statement carries neither and an ordered update does not typecheck:
///
/// ```compile_fail,E0599
/// use pgorm_query::{tests_cfg::*, *};
///
/// Query::update()
///     .table(Glyph::Table)
///     .value(Glyph::Aspect, 1)
///     .order_by(Glyph::Id, Order::Asc);
/// ```
///
/// nor a limited one:
///
/// ```compile_fail,E0599
/// use pgorm_query::{tests_cfg::*, *};
///
/// Query::update()
///     .table(Glyph::Table)
///     .value(Glyph::Aspect, 1)
///     .limit(1);
/// ```
///
/// Both belong to the SELECT that chooses the rows, so an update over an
/// ordered, limited set is spelled as a subquery filter:
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let query = Query::update()
///     .table(Glyph::Table)
///     .value(Glyph::Aspect, 1)
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
///     r#"UPDATE "glyph" SET "aspect" = 1 WHERE "id" IN (SELECT "id" FROM "glyph" ORDER BY "id" ASC LIMIT 1)"#
/// );
/// ```
// [spec:pgorm:req:sql.ast.update+2]
#[derive(Default, Debug, Clone, PartialEq)]
pub struct UpdateStatement {
    pub(crate) table: Option<NamedTable>,
    pub(crate) values: Vec<(DynIden, Box<SimpleExpr>)>,
    pub(crate) r#where: ConditionHolder,
    pub(crate) returning: Option<ReturningClause>,
}

impl UpdateStatement {
    /// Construct a new [`UpdateStatement`]
    pub fn new() -> Self {
        Self::default()
    }

    /// Specify which table to update.
    ///
    /// The target is a name, optionally aliased:
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::update()
    ///     .table(Glyph::Table.into_named_table().alias(Alias::new("g")))
    ///     .value(Glyph::Aspect, 1.23)
    ///     .and_where(Expr::col((Alias::new("g"), Glyph::Id)).eq(1))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"UPDATE "glyph" AS "g" SET "aspect" = 1.23 WHERE "g"."id" = 1"#
    /// );
    /// ```
    ///
    /// # Examples
    ///
    /// See [`UpdateStatement::values`]
    // [spec:pgorm:req:sql.ast.update+2]
    #[allow(clippy::wrong_self_convention)]
    pub fn table<T>(&mut self, tbl_ref: T) -> &mut Self
    where
        T: IntoNamedTable,
    {
        self.table = Some(tbl_ref.into_named_table());
        self
    }

    /// Update column values. To set multiple column-value pairs at once.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::update()
    ///     .table(Glyph::Table)
    ///     .values([
    ///         (Glyph::Aspect, 2.1345.into()),
    ///         (Glyph::Image, "235m".into()),
    ///     ])
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"UPDATE "glyph" SET "aspect" = 2.1345, "image" = '235m'"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.update+2]
    pub fn values<T, I>(&mut self, values: I) -> &mut Self
    where
        T: IntoIden,
        I: IntoIterator<Item = (T, SimpleExpr)>,
    {
        for (k, v) in values.into_iter() {
            self.values.push((k.into_iden(), Box::new(v)));
        }
        self
    }

    /// Update column value by [`SimpleExpr`].
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::update()
    ///     .table(Glyph::Table)
    ///     .value(Glyph::Aspect, Expr::cust("60 * 24 * 24"))
    ///     .values([
    ///         (Glyph::Image, "24B0E11951B03B07F8300FD003983F03F0780060".into()),
    ///     ])
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"UPDATE "glyph" SET "aspect" = 60 * 24 * 24, "image" = '24B0E11951B03B07F8300FD003983F03F0780060'"#
    /// );
    /// ```
    pub fn value<C, T>(&mut self, col: C, value: T) -> &mut Self
    where
        C: IntoIden,
        T: Into<SimpleExpr>,
    {
        self.values.push((col.into_iden(), Box::new(value.into())));
        self
    }

    /// RETURNING expressions.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::update()
    ///     .table(Glyph::Table)
    ///     .value(Glyph::Aspect, 2.1345)
    ///     .value(Glyph::Image, "235m")
    ///     .returning(Query::returning().columns([Glyph::Id]))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"UPDATE "glyph" SET "aspect" = 2.1345, "image" = '235m' RETURNING "id""#
    /// );
    /// ```
    pub fn returning(&mut self, returning: ReturningClause) -> &mut Self {
        self.returning = Some(returning);
        self
    }

    /// RETURNING expressions for a column.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::update()
    ///     .table(Glyph::Table)
    ///     .table(Glyph::Table)
    ///     .value(Glyph::Aspect, 2.1345)
    ///     .value(Glyph::Image, "235m")
    ///     .returning_col(Glyph::Id)
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"UPDATE "glyph" SET "aspect" = 2.1345, "image" = '235m' RETURNING "id""#
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
    /// let query = Query::update()
    ///     .table(Glyph::Table)
    ///     .table(Glyph::Table)
    ///     .value(Glyph::Aspect, 2.1345)
    ///     .value(Glyph::Image, "235m")
    ///     .returning_all()
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"UPDATE "glyph" SET "aspect" = 2.1345, "image" = '235m' RETURNING *"#
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
    ///     let update = UpdateStatement::new()
    ///         .table(Glyph::Table)
    ///         .and_where(Expr::col(Glyph::Id).in_subquery(SelectStatement::new().column(Glyph::Id).from(Alias::new("cte")).to_owned()))
    ///         .value(Glyph::Aspect, Expr::cust("60 * 24 * 24"))
    ///         .to_owned();
    ///     let query = update.with(with_clause);
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"WITH "cte" ("id") AS (SELECT "id" FROM "glyph" WHERE "image" LIKE '0%') UPDATE "glyph" SET "aspect" = 60 * 24 * 24 WHERE "id" IN (SELECT "id" FROM "cte")"#
    /// );
    /// ```
    pub fn with<C>(self, clause: C) -> WithQuery
    where
        C: Into<AnyWithClause>,
    {
        WithQuery::new(clause, self)
    }

    /// Get column values
    pub fn get_values(&self) -> &[(DynIden, Box<SimpleExpr>)] {
        &self.values
    }
}

#[inherent]
impl QueryStatementBuilder for UpdateStatement {
    pub fn build_collect_into(&self, sql: &mut dyn SqlWriter) {
        QueryBuilder.prepare_update_statement(self, sql);
    }

    pub fn into_sub_query_statement(self) -> SubQueryStatement {
        SubQueryStatement::UpdateStatement(self)
    }

    pub fn build(&self) -> (String, Values);
    pub fn build_collect(&self, sql: &mut dyn SqlWriter) -> String;
}

// [spec:pgorm:req:sql.ast.build+1] (the one value-inlined rendering)
impl std::fmt::Display for UpdateStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sql = String::with_capacity(256);
        QueryBuilder.prepare_update_statement(self, &mut sql);
        f.write_str(&sql)
    }
}

#[inherent]
impl ConditionalStatement for UpdateStatement {
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
