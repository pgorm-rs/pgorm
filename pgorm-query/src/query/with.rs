use crate::{
    ColumnRef, DynIden, IntoIden, QueryStatementBuilder, QueryStatementWriter, SelectExpr,
    SelectStatement, SimpleExpr, SqlWriter, SubQueryStatement, TableRef, Values,
    {Alias, QueryBuilder},
};
use inherent::inherent;

/// A table definition inside a WITH clause ([WithClause] or [RecursiveWithClause]).
///
/// These named queries can act as a "query local table" that are materialized during execution and
/// then can be used by the query prefixed with the WITH clause.
///
/// A [CommonTableExpression] is a name, column names and a query returning data for those columns.
/// The name and the query are given to [CommonTableExpression::new]; the column list and the
/// materialization hint are optional and are added afterwards.
///
/// Some databases (like sqlite) restrict the acceptable kinds of queries inside of the WITH clause
/// common table expressions. These databases only allow [SelectStatement]s to form a common table
/// expression.
///
/// Other databases like postgres allow modification queries (UPDATE, DELETE) inside of the WITH
/// clause but they have to return a table. (They must have a RETURNING clause).
///
/// pgorm-query doesn't check this or restrict the kind of [CommonTableExpression] that you can create
/// in rust. This means that you can put an UPDATE or DELETE queries into WITH clause and pgorm-query
/// will succeed in generating that kind of sql query but the execution inside the database will
/// fail because they are invalid.
///
/// It is your responsibility to ensure that the kind of WITH clause that you put together makes
/// sense and valid for that database that you are using.
// [spec:pgorm:def:sql.ast.with+1]
#[derive(Debug, Clone, PartialEq)]
pub struct CommonTableExpression {
    pub(crate) table_name: DynIden,
    pub(crate) cols: Vec<DynIden>,
    pub(crate) query: Box<SubQueryStatement>,
    pub(crate) materialized: Option<bool>,
}

impl CommonTableExpression {
    /// Construct a new [`CommonTableExpression`] from the two mandatory parts: the CTE table name
    /// and the query producing its rows.
    pub fn new<T, Q>(table_name: T, query: Q) -> Self
    where
        T: IntoIden,
        Q: QueryStatementBuilder,
    {
        Self {
            table_name: table_name.into_iden(),
            cols: Vec::new(),
            query: Box::new(query.into_sub_query_statement()),
            materialized: None,
        }
    }

    /// Adds a named column to the CTE table definition.
    pub fn column<C>(&mut self, col: C) -> &mut Self
    where
        C: IntoIden,
    {
        self.cols.push(col.into_iden());
        self
    }

    /// Adds a named columns to the CTE table definition.
    pub fn columns<T, I>(&mut self, cols: I) -> &mut Self
    where
        T: IntoIden,
        I: IntoIterator<Item = T>,
    {
        self.cols
            .extend(cols.into_iter().map(|col| col.into_iden()));
        self
    }

    /// Some databases allow you to put "MATERIALIZED" or "NOT MATERIALIZED" in the CTE definition.
    /// This will affect how during the execution of [WithQuery] the CTE in the with clause will be
    /// executed. If the database doesn't support this syntax this option specified here will be
    /// ignored and not appear in the generated sql.
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

    fn derived_table_name(from: &TableRef) -> Option<DynIden> {
        let iden = match from {
            TableRef::Table(iden)
            | TableRef::SchemaTable(_, iden)
            | TableRef::DatabaseSchemaTable(_, _, iden)
            | TableRef::TableAlias(_, iden)
            | TableRef::SchemaTableAlias(_, _, iden)
            | TableRef::DatabaseSchemaTableAlias(_, _, _, iden) => iden,
            _ => return None,
        };

        Some(Alias::new(format!("cte_{}", iden.to_string())).into_iden())
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

    fn cols_from_selects(selects: &[SelectExpr]) -> Option<Vec<DynIden>> {
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
                                    .into_iden(),
                            ),
                            ColumnRef::SchemaTableColumn(schema, table, column) => Some(
                                Alias::new(format!(
                                    "{}_{}_{}",
                                    schema.to_string(),
                                    table.to_string(),
                                    column.to_string()
                                ))
                                .into_iden(),
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
    pub(crate) alias: DynIden,
}

impl Search {
    /// Create a [Search] specification from the traversal order, the expression tracking the path
    /// in the graph, and the name of the order column generated by this clause. That name is what
    /// you can use to order the result of the [CommonTableExpression].
    pub fn new<E, A>(order: SearchOrder, expr: E, alias: A) -> Self
    where
        E: Into<SimpleExpr>,
        A: IntoIden,
    {
        Self {
            order,
            expr: expr.into(),
            alias: alias.into_iden(),
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
    pub(crate) set_as: DynIden,
    pub(crate) using: DynIden,
}

impl Cycle {
    /// Create a [Cycle] specification from the expression identifying nodes, the name of the
    /// boolean column containing whether we have completed a cycle or not yet, and the name of the
    /// array typed column that contains the node ids (generated using the expression) that specify
    /// the current nodes path. Both columns are generated by this clause.
    pub fn new<E, S, U>(expr: E, set: S, using: U) -> Self
    where
        E: Into<SimpleExpr>,
        S: IntoIden,
        U: IntoIden,
    {
        Self {
            expr: expr.into(),
            set_as: set.into_iden(),
            using: using.into_iden(),
        }
    }
}

/// A non-recursive WITH clause: one or more common table expressions ([CommonTableExpression]).
///
/// The first CTE is given to [WithClause::new] and further ones are appended with
/// [WithClause::cte], so the clause is never empty. The recursive form is the separate
/// [RecursiveWithClause].
///
/// You can use this to generate [WithQuery] by calling [WithClause::query].
///
/// These named queries can act as a "query local table" that are materialized during execution and
/// then can be used by the query prefixed with the WITH clause.
///
/// Some databases (like sqlite) restrict the acceptable kinds of queries inside of the WITH clause
/// common table expressions. These databases only allow [SelectStatement]s to form a common table
/// expression.
///
/// Other databases like postgres allow modification queries (UPDATE, DELETE) inside of the WITH
/// clause but they have to return a table. (They must have a RETURNING clause).
///
/// pgorm-query doesn't check this or restrict the kind of [CommonTableExpression] that you can create
/// in rust. This means that you can put an UPDATE or DELETE queries into WITH clause and pgorm-query
/// will succeed in generating that kind of sql query but the execution inside the database will
/// fail because they are invalid.
///
/// It is your responsibility to ensure that the kind of WITH clause that you put together makes
/// sense and valid for that database that you are using.
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
/// let select = SelectStatement::new()
///         .column(ColumnRef::Asterisk)
///         .from(Alias::new("cte"))
///         .to_owned();
///
/// let query = select.with(WithClause::new(common_table_expression));
///
/// assert_eq!(
///     query.to_string(QueryBuilder),
///     r#"WITH "cte" ("id") AS (SELECT "id" FROM "table") SELECT * FROM "cte""#
/// );
/// ```
// [spec:pgorm:def:sql.ast.with+1]
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

    /// You can turn this into a [WithQuery] using this function. The resulting WITH query will
    /// execute the argument query with this WITH clause.
    pub fn query<T>(self, query: T) -> WithQuery
    where
        T: QueryStatementBuilder,
    {
        WithQuery::new(self, query)
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
/// use pgorm_query::{*, IntoIden, tests_cfg::*};
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
/// let select = SelectStatement::new()
///         .column(ColumnRef::Asterisk)
///         .from(Alias::new("cte_traversal"))
///         .to_owned();
///
/// let with_clause = RecursiveWithClause::new(common_table_expression)
///         .cycle(Cycle::new(SimpleExpr::Column(ColumnRef::Column(Alias::new("id").into_iden())), Alias::new("looped"), Alias::new("traversal_path")))
///         .to_owned();
///
/// let query = select.with(with_clause);
///
/// assert_eq!(
///     query.to_string(QueryBuilder),
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

    /// You can turn this into a [WithQuery] using this function. The resulting WITH query will
    /// execute the argument query with this WITH clause.
    pub fn query<T>(self, query: T) -> WithQuery
    where
        T: QueryStatementBuilder,
    {
        WithQuery::new(self, query)
    }
}

/// Either form of WITH clause. This is what the statement builders' `with` methods accept and what
/// a [WithQuery] carries.
// [spec:pgorm:def:sql.ast.with+1]
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

/// A WITH query. A simple SQL query that has a WITH clause ([WithClause] or
/// [RecursiveWithClause]).
///
/// These named queries can act as a "query local table" that are materialized during execution and
/// then can be used by the query prefixed with the WITH clause.
///
/// Both the clause and the query it prefixes are given to [WithQuery::new], so a [WithQuery] is
/// always complete. It is usually built through [WithClause::query],
/// [RecursiveWithClause::query], or the `with` method on a select/insert/update/delete statement.
// [spec:pgorm:def:sql.ast.with+1]
#[derive(Debug, Clone, PartialEq)]
pub struct WithQuery {
    pub(crate) with_clause: AnyWithClause,
    pub(crate) query: Box<SubQueryStatement>,
}

impl WithQuery {
    /// Constructs a [WithQuery] from a with clause of either form and the query it prefixes.
    pub fn new<C, T>(with_clause: C, query: T) -> Self
    where
        C: Into<AnyWithClause>,
        T: QueryStatementBuilder,
    {
        Self {
            with_clause: with_clause.into(),
            query: Box::new(query.into_sub_query_statement()),
        }
    }
}

impl QueryStatementBuilder for WithQuery {
    fn build_collect_any_into(&self, query_builder: &QueryBuilder, sql: &mut dyn SqlWriter) {
        query_builder.prepare_with_query(self, sql);
    }

    fn into_sub_query_statement(self) -> SubQueryStatement {
        SubQueryStatement::WithStatement(self)
    }
}

#[inherent]
impl QueryStatementWriter for WithQuery {
    pub fn build_collect_into(&self, query_builder: QueryBuilder, sql: &mut dyn SqlWriter) {
        query_builder.prepare_with_query(self, sql);
    }

    pub fn build_collect(&self, query_builder: QueryBuilder, sql: &mut dyn SqlWriter) -> String;
    pub fn build(&self, query_builder: QueryBuilder) -> (String, Values);
    pub fn to_string(&self, query_builder: QueryBuilder) -> String;
}
