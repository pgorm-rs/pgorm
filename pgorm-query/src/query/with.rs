use crate::{
    Alias, ColumnRef, DeleteStatement, FromItem, InsertStatement, IntoName, Name,
    QueryStatementBuilder, SelectExpr, SelectStatement, SimpleExpr, SubQueryStatement,
    UpdateStatement,
};

/// A table definition inside a WITH clause ([WithClause] or [RecursiveWithClause]).
///
/// These named queries can act as a "query local table" that are materialized during execution and
/// then can be used by the query prefixed with the WITH clause.
///
/// A [CommonTableExpression] is a name, column names and a query returning data for those columns.
/// The name and the query are given to [CommonTableExpression::new]; the column list and the
/// materialization hint are optional and are added afterwards.
///
/// PostgreSQL admits a data-modifying statement — INSERT, UPDATE, DELETE — as a
/// common table expression, provided it carries a RETURNING clause so it yields
/// the rows the enclosing query reads.
///
/// pgorm-query does not enforce that: a write CTE with no RETURNING renders
/// happily and is refused by the server. Supplying the RETURNING clause is the
/// caller's part.
// [spec:pgorm:def:sql.ast.with+3]
#[derive(Debug, Clone, PartialEq)]
pub struct CommonTableExpression {
    pub(crate) table_name: Name,
    pub(crate) cols: Vec<Name>,
    pub(crate) query: Box<SubQueryStatement>,
    pub(crate) materialized: Option<bool>,
}

impl CommonTableExpression {
    /// Construct a new [`CommonTableExpression`] from the two mandatory parts: the CTE table name
    /// and the query producing its rows.
    pub fn new<T, Q>(table_name: T, query: Q) -> Self
    where
        T: IntoName,
        Q: QueryStatementBuilder,
    {
        Self {
            table_name: table_name.into_name(),
            cols: Vec::new(),
            query: Box::new(query.into_sub_query_statement()),
            materialized: None,
        }
    }

    /// Adds a named column to the CTE table definition.
    pub fn column<C>(&mut self, col: C) -> &mut Self
    where
        C: IntoName,
    {
        self.cols.push(col.into_name());
        self
    }

    /// Adds a named columns to the CTE table definition.
    pub fn columns<T, I>(&mut self, cols: I) -> &mut Self
    where
        T: IntoName,
        I: IntoIterator<Item = T>,
    {
        self.cols
            .extend(cols.into_iter().map(|col| col.into_name()));
        self
    }

    /// Some databases allow you to put "MATERIALIZED" or "NOT MATERIALIZED" in the CTE definition.
    /// This affects how the CTE is executed when the statement carrying the clause runs. If the
    /// database doesn't support this syntax this option specified here will be ignored and not
    /// appear in the generated sql.
    pub fn materialized(&mut self, materialized: bool) -> &mut Self {
        self.materialized = Some(materialized);
        self
    }

    /// Create a CTE from a [SelectStatement], naming it `cte_<table>` after the first table in the
    /// select's FROM clause. If the selections are named columns then the returned
    /// [CommonTableExpression] has the column names set.
    ///
    /// Returns [None] when the select has no FROM table to take a name from, since a CTE without a
    /// name cannot be rendered.
    pub fn from_select(select: SelectStatement) -> Option<Self> {
        let table_name = select.from.first().and_then(Self::derived_table_name)?;
        let cols = Self::cols_from_selects(&select.selects).unwrap_or_default();

        Some(Self {
            table_name,
            cols,
            query: Box::new(select.into_sub_query_statement()),
            materialized: None,
        })
    }

    fn derived_table_name(from: &FromItem) -> Option<Name> {
        let FromItem::Table(table) = from else {
            return None;
        };

        Some(Alias::new(format!("cte_{}", table.qualifier().to_string())).into_name())
    }

    /// Set up the columns of the CTE to match the given [SelectStatement] selected columns.
    /// This will fail if the select contains non named columns like expressions of wildcards.
    ///
    /// Returns true if the column setup from the select query was successful. If the returned
    /// value is false the columns are untouched.
    pub fn try_set_cols_from_select(&mut self, select: &SelectStatement) -> bool {
        match Self::cols_from_selects(&select.selects) {
            Some(cols) => {
                self.cols = cols;
                true
            }
            None => false,
        }
    }

    fn cols_from_selects(selects: &[SelectExpr]) -> Option<Vec<Name>> {
        selects
            .iter()
            .map(|select| {
                if let Some(ident) = &select.alias {
                    Some(ident.clone())
                } else {
                    match &select.expr {
                        SimpleExpr::Column(column) => match column {
                            ColumnRef::Column(iden) => Some(iden.clone()),
                            ColumnRef::TableColumn(table, column) => Some(
                                Alias::new(format!("{}_{}", table.to_string(), column.to_string()))
                                    .into_name(),
                            ),
                            ColumnRef::SchemaTableColumn(schema, table, column) => Some(
                                Alias::new(format!(
                                    "{}_{}_{}",
                                    schema.to_string(),
                                    table.to_string(),
                                    column.to_string()
                                ))
                                .into_name(),
                            ),
                            _ => None,
                        },
                        _ => None,
                    }
                }
            })
            .collect()
    }
}

/// For [RecursiveWithClause]s the traversing order can be specified in some databases
/// that support this functionality.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchOrder {
    /// Breadth first traversal during the execution of the recursive query.
    BREADTH,
    /// Depth first traversal during the execution of the recursive query.
    DEPTH,
}

/// For [RecursiveWithClause]s the traversing order can be specified in some databases
/// that support this functionality.
///
/// The clause contains the type of traversal ([SearchOrder]), the expression that is used to
/// construct the current path, and the name of the order column this clause generates. All three
/// are given to [Search::new], so a [Search] is always complete.
///
/// A query can have both SEARCH and CYCLE clauses.
// [spec:pgorm:req:sql.ast.with.recursive+1]
#[derive(Debug, Clone, PartialEq)]
pub struct Search {
    pub(crate) order: SearchOrder,
    pub(crate) expr: SimpleExpr,
    pub(crate) alias: Name,
}

impl Search {
    /// Create a [Search] specification from the traversal order, the expression tracking the path
    /// in the graph, and the name of the order column generated by this clause. That name is what
    /// you can use to order the result of the [CommonTableExpression].
    pub fn new<E, A>(order: SearchOrder, expr: E, alias: A) -> Self
    where
        E: Into<SimpleExpr>,
        A: IntoName,
    {
        Self {
            order,
            expr: expr.into(),
            alias: alias.into_name(),
        }
    }
}

/// For [RecursiveWithClause]s the CYCLE sql clause can be specified to avoid creating
/// an infinite traversals that loops on graph cycles indefinitely. You specify an expression that
/// identifies a node in the graph and that will be used to determine during the iteration of
/// the execution of the query when appending of new values whether the new values are distinct new
/// nodes or are already visited and therefore they should be added again into the result.
///
/// A query can have both SEARCH and CYCLE clauses.
///
/// The expression, the cycle mark column and the path column are all given to [Cycle::new], so a
/// [Cycle] is always complete.
// [spec:pgorm:req:sql.ast.with.recursive+1]
#[derive(Debug, Clone, PartialEq)]
pub struct Cycle {
    pub(crate) expr: SimpleExpr,
    pub(crate) set_as: Name,
    pub(crate) using: Name,
}

impl Cycle {
    /// Create a [Cycle] specification from the expression identifying nodes, the name of the
    /// boolean column containing whether we have completed a cycle or not yet, and the name of the
    /// array typed column that contains the node ids (generated using the expression) that specify
    /// the current nodes path. Both columns are generated by this clause.
    pub fn new<E, S, U>(expr: E, set: S, using: U) -> Self
    where
        E: Into<SimpleExpr>,
        S: IntoName,
        U: IntoName,
    {
        Self {
            expr: expr.into(),
            set_as: set.into_name(),
            using: using.into_name(),
        }
    }
}

/// A non-recursive WITH clause: one or more common table expressions ([CommonTableExpression]).
///
/// The first CTE is given to [WithClause::new] and further ones are appended with
/// [WithClause::cte], so the clause is never empty. The recursive form is the separate
/// [RecursiveWithClause].
///
/// Attach it to a statement with that statement's `with` method — see
/// [`SelectStatement::with`].
///
/// These named queries can act as a "query local table" that are materialized during execution and
/// then can be used by the query prefixed with the WITH clause.
///
/// PostgreSQL admits a data-modifying statement — INSERT, UPDATE, DELETE — as a
/// common table expression, provided it carries a RETURNING clause so it yields
/// the rows the enclosing query reads.
///
/// pgorm-query does not enforce that: a write CTE with no RETURNING renders
/// happily and is refused by the server. Supplying the RETURNING clause is the
/// caller's part.
///
/// # Examples
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let common_table_expression = CommonTableExpression::new(
///         Alias::new("cte"),
///         SelectStatement::new()
///             .column(Alias::new("id"))
///             .from(Alias::new("table"))
///             .to_owned(),
///     )
///     .column(Alias::new("id"))
///     .to_owned();
///
/// let query = SelectStatement::new()
///         .column(ColumnRef::Asterisk)
///         .from(Alias::new("cte"))
///         .with(WithClause::new(common_table_expression))
///         .to_owned();
///
/// assert_eq!(
///     query.to_string(),
///     r#"WITH "cte" ("id") AS (SELECT "id" FROM "table") SELECT * FROM "cte""#
/// );
/// ```
// [spec:pgorm:def:sql.ast.with+3]
#[derive(Debug, Clone, PartialEq)]
pub struct WithClause {
    pub(crate) first: CommonTableExpression,
    pub(crate) rest: Vec<CommonTableExpression>,
}

impl WithClause {
    /// Constructs a new [WithClause] around its first [CommonTableExpression].
    pub fn new(cte: CommonTableExpression) -> Self {
        Self {
            first: cte,
            rest: Vec::new(),
        }
    }

    /// Add another [CommonTableExpression] to this with clause.
    pub fn cte(&mut self, cte: CommonTableExpression) -> &mut Self {
        self.rest.push(cte);
        self
    }

    /// The common table expressions of this clause, in the order they were added.
    pub fn ctes(&self) -> impl Iterator<Item = &CommonTableExpression> {
        std::iter::once(&self.first).chain(self.rest.iter())
    }
}

/// A recursive WITH clause ("WITH RECURSIVE"): exactly one [CommonTableExpression], plus the
/// optional SEARCH ([Search]) and CYCLE ([Cycle]) clauses that only this form accepts.
///
/// The single query must match certain requirements:
///   * It is a query of UNION or UNION ALL of two queries.
///   * The first part of the query (the left side of the UNION) must be executable first in itself.
///     It must be non-recursive. (Cannot contain self reference)
///   * The self reference must appear in the right hand side of the UNION.
///   * The query can only have a single self-reference.
///   * Recursive data-modifying statements are not supported, but you can use the results of a
///     recursive SELECT query in a data-modifying statement. (like so: WITH RECURSIVE
///     cte_name(a,b,c,d) AS (SELECT ... UNION SELECT ... FROM ... JOIN cte_name ON ... WHERE ...)
///     DELETE FROM table WHERE table.a = cte_name.a)
///
/// pgorm-query does not check these; it is your responsibility to ensure the recursive query you
/// put together is valid for the database that you are using.
///
/// # Examples
///
/// ```
/// use pgorm_query::{*, IntoName, tests_cfg::*};
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
///         Alias::new("cte_traversal"),
///         base_query.clone().union(UnionType::All, cte_referencing).to_owned(),
///     )
///     .column(Alias::new("id"))
///     .column(Alias::new("depth"))
///     .column(Alias::new("next"))
///     .column(Alias::new("value"))
///     .to_owned();
///
/// let with_clause = RecursiveWithClause::new(common_table_expression)
///         .cycle(Cycle::new(SimpleExpr::Column(ColumnRef::Column(Alias::new("id").into_name())), Alias::new("looped"), Alias::new("traversal_path")))
///         .to_owned();
///
/// let query = SelectStatement::new()
///         .column(ColumnRef::Asterisk)
///         .from(Alias::new("cte_traversal"))
///         .with(with_clause)
///         .to_owned();
///
/// assert_eq!(
///     query.to_string(),
///     r#"WITH RECURSIVE "cte_traversal" ("id", "depth", "next", "value") AS (SELECT "id", 1, "next", "value" FROM "table" UNION ALL (SELECT "id", "depth" + 1, "next", "value" FROM "table" INNER JOIN "cte_traversal" ON "cte_traversal"."next" = "table"."id")) CYCLE "id" SET "looped" USING "traversal_path" SELECT * FROM "cte_traversal""#
/// );
/// ```
// [spec:pgorm:req:sql.ast.with.recursive+1]
#[derive(Debug, Clone, PartialEq)]
pub struct RecursiveWithClause {
    pub(crate) cte: CommonTableExpression,
    pub(crate) search: Option<Search>,
    pub(crate) cycle: Option<Cycle>,
}

impl RecursiveWithClause {
    /// Constructs a new [RecursiveWithClause] around the single [CommonTableExpression] a
    /// recursive WITH query is allowed to have.
    pub fn new(cte: CommonTableExpression) -> Self {
        Self {
            cte,
            search: None,
            cycle: None,
        }
    }

    /// Specify the [Search] clause.
    ///
    /// Some databases don't support this clause. In that case this option will be silently ignored.
    pub fn search(&mut self, search: Search) -> &mut Self {
        self.search = Some(search);
        self
    }

    /// Specify the [Cycle] clause.
    ///
    /// Some databases don't support this clause. In that case this option will be silently ignored.
    pub fn cycle(&mut self, cycle: Cycle) -> &mut Self {
        self.cycle = Some(cycle);
        self
    }
}

/// Either form of WITH clause. This is what every statement builder's `with`
/// method accepts, and what the statement then carries.
// [spec:pgorm:def:sql.ast.with+3]
#[derive(Debug, Clone, PartialEq)]
pub enum AnyWithClause {
    /// A non-recursive clause of one or more common table expressions.
    Plain(WithClause),
    /// A recursive clause of exactly one common table expression.
    Recursive(RecursiveWithClause),
}

impl From<WithClause> for AnyWithClause {
    fn from(clause: WithClause) -> Self {
        Self::Plain(clause)
    }
}

impl From<RecursiveWithClause> for AnyWithClause {
    fn from(clause: RecursiveWithClause) -> Self {
        Self::Recursive(clause)
    }
}

impl SelectStatement {
    /// Attach a WITH clause — either a [`WithClause`] or a
    /// [`RecursiveWithClause`] — to this statement.
    ///
    /// The clause is carried *on* the statement rather than wrapping it, so the
    /// value stays a [`SelectStatement`]: `and_where`, `order_by`, `limit` and
    /// every other builder method still apply afterwards, and the statement
    /// still nests as a subquery, a union arm, a CTE body or a LATERAL body.
    /// The clause renders as a prefix at whatever level the statement occupies.
    ///
    /// [`InsertStatement`], [`UpdateStatement`] and [`DeleteStatement`] carry a
    /// clause the same way, through a method of the same name, receiver and
    /// return: `with` means one thing across all four.
    ///
    /// The last call wins; a statement carries at most one clause.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, IntoCondition, IntoName, tests_cfg::*};
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
    /// let query = SelectStatement::new()
    ///         .column(ColumnRef::Asterisk)
    ///         .from(Alias::new("cte_traversal"))
    ///         .with(RecursiveWithClause::new(common_table_expression))
    ///         .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"WITH RECURSIVE "cte_traversal" ("id", "depth", "next", "value") AS (SELECT "id", 1, "next", "value" FROM "table" UNION ALL (SELECT "id", "depth" + 1, "next", "value" FROM "table" INNER JOIN "cte_traversal" ON "cte_traversal"."next" = "table"."id")) SELECT * FROM "cte_traversal""#
    /// );
    /// ```
    // [spec:pgorm:def:query.build.with+1]
    // [spec:pgorm:sem:query.build.with.attach+1]
    // [spec:pgorm:req:query.build.with.single+1]
    pub fn with<C>(&mut self, clause: C) -> &mut Self
    where
        C: Into<AnyWithClause>,
    {
        self.with = Some(Box::new(clause.into()));
        self
    }
}

impl InsertStatement {
    /// Attach a WITH clause to this statement. See
    /// [`SelectStatement::with`] — the method is the same one, on every
    /// statement that has it.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let cte = CommonTableExpression::new(
    ///     Alias::new("cte"),
    ///     Query::select()
    ///         .columns([Glyph::Id, Glyph::Image, Glyph::Aspect])
    ///         .from(Glyph::Table)
    ///         .to_owned(),
    /// )
    /// .columns([Glyph::Id, Glyph::Image, Glyph::Aspect])
    /// .to_owned();
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Id, Glyph::Image, Glyph::Aspect])
    ///     .select_from(
    ///         Query::select()
    ///             .columns([Glyph::Id, Glyph::Image, Glyph::Aspect])
    ///             .from(Alias::new("cte"))
    ///             .to_owned(),
    ///     )
    ///     .unwrap()
    ///     .with(WithClause::new(cte))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"WITH "cte" ("id", "image", "aspect") AS (SELECT "id", "image", "aspect" FROM "glyph") INSERT INTO "glyph" ("id", "image", "aspect") SELECT "id", "image", "aspect" FROM "cte""#
    /// );
    /// ```
    // [spec:pgorm:def:query.build.with+1]
    // [spec:pgorm:sem:query.build.with.attach+1]
    // [spec:pgorm:req:query.build.with.single+1]
    pub fn with<C>(&mut self, clause: C) -> &mut Self
    where
        C: Into<AnyWithClause>,
    {
        self.with = Some(Box::new(clause.into()));
        self
    }
}

impl UpdateStatement {
    /// Attach a WITH clause to this statement. See
    /// [`SelectStatement::with`] — the method is the same one, on every
    /// statement that has it.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let cte = CommonTableExpression::new(
    ///     Alias::new("cte"),
    ///     Query::select().column(Glyph::Id).from(Glyph::Table).to_owned(),
    /// )
    /// .column(Glyph::Id)
    /// .to_owned();
    ///
    /// let query = Query::update()
    ///     .table(Glyph::Table)
    ///     .value(Glyph::Aspect, 2.1345)
    ///     .and_where(Expr::col(Glyph::Id).in_subquery(
    ///         Query::select().column(Glyph::Id).from(Alias::new("cte")).to_owned(),
    ///     ))
    ///     .with(WithClause::new(cte))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"WITH "cte" ("id") AS (SELECT "id" FROM "glyph") UPDATE "glyph" SET "aspect" = 2.1345 WHERE "id" IN (SELECT "id" FROM "cte")"#
    /// );
    /// ```
    // [spec:pgorm:def:query.build.with+1]
    // [spec:pgorm:sem:query.build.with.attach+1]
    // [spec:pgorm:req:query.build.with.single+1]
    pub fn with<C>(&mut self, clause: C) -> &mut Self
    where
        C: Into<AnyWithClause>,
    {
        self.with = Some(Box::new(clause.into()));
        self
    }
}

impl DeleteStatement {
    /// Attach a WITH clause to this statement. See
    /// [`SelectStatement::with`] — the method is the same one, on every
    /// statement that has it.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let cte = CommonTableExpression::new(
    ///     Alias::new("cte"),
    ///     Query::select().column(Glyph::Id).from(Glyph::Table).to_owned(),
    /// )
    /// .column(Glyph::Id)
    /// .to_owned();
    ///
    /// let query = Query::delete()
    ///     .from_table(Glyph::Table)
    ///     .and_where(Expr::col(Glyph::Id).in_subquery(
    ///         Query::select().column(Glyph::Id).from(Alias::new("cte")).to_owned(),
    ///     ))
    ///     .with(WithClause::new(cte))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"WITH "cte" ("id") AS (SELECT "id" FROM "glyph") DELETE FROM "glyph" WHERE "id" IN (SELECT "id" FROM "cte")"#
    /// );
    /// ```
    // [spec:pgorm:def:query.build.with+1]
    // [spec:pgorm:sem:query.build.with.attach+1]
    // [spec:pgorm:req:query.build.with.single+1]
    pub fn with<C>(&mut self, clause: C) -> &mut Self
    where
        C: Into<AnyWithClause>,
    {
        self.with = Some(Box::new(clause.into()));
        self
    }
}
