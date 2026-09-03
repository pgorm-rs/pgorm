use crate::{
    ConnectionTrait, EntityTrait, FromQueryResult, Select, SelectModel, SelectTwo, SelectTwoModel,
    Selector, SelectorRaw, SelectorTrait, error::*,
};
use async_stream::stream;
use futures::Stream;
use pg_query::{NodeEnum, protobuf::RawStmt};
use pgorm_query::{Alias, Expr, SelectStatement, Value};
use std::{
    fmt::{self, Write as _},
    marker::PhantomData,
    num::NonZeroU64,
    pin::Pin,
};
use tokio_postgres::types::ToSql;

use super::{QueryResult, ValueHolder, select::ensure_select_list};

/// Pin a Model so that stream operations can be performed on the model
pub type PinBoxStream<'db, Item> = Pin<Box<dyn Stream<Item = Item> + 'db>>;

/// Defined a structure to handle pagination of a result from a query operation on a Model
// [spec:pgorm:def:exec.paginator+1]
#[derive(Clone, Debug)]
pub struct Paginator<'db, C, S>
where
    C: ConnectionTrait,
    S: SelectorTrait + 'db,
{
    pub(crate) query: Result<SelectStatement, String>,
    pub(crate) page: u64,
    pub(crate) page_size: NonZeroU64,
    pub(crate) db: &'db C,
    pub(crate) selector: PhantomData<S>,
}

/// Define a structure containing the numbers of items and pages of a Paginator
#[derive(Clone, Debug)]
pub struct ItemsAndPagesNumber {
    /// The total number of items of a paginator
    pub number_of_items: u64,
    /// The total number of pages of a paginator
    pub number_of_pages: u64,
}

// LINT: warn if paginator is used without an order by clause

impl<'db, C, S> Paginator<'db, C, S>
where
    C: ConnectionTrait,
    S: SelectorTrait + 'db,
{
    /// The statement to page over, or the reason there is none to page over.
    // [spec:pgorm:sem:exec.paginator.raw+1]
    fn query(&self) -> Result<&SelectStatement, DbErr> {
        let query = self
            .query
            .as_ref()
            .map_err(|report| DbErr::Query(RuntimeErr::Internal(report.clone())))?;
        ensure_select_list(query)?;
        Ok(query)
    }

    /// Fetch a specific page; page index starts from zero
    // [spec:pgorm:sem:exec.paginator.fetch+1]
    pub async fn fetch_page(&self, page: u64) -> Result<Vec<S::Item>, DbErr> {
        let offset = self.page_size.get().checked_mul(page).ok_or_else(|| {
            DbErr::Query(RuntimeErr::Internal(format!(
                "page {page} at page size {} is past the largest representable offset",
                self.page_size
            )))
        })?;
        let mut query = self.query()?.clone();
        query.limit(self.page_size.get()).offset(offset);
        let (stmt, values) = query.build();
        let values = values.into_iter().map(ValueHolder).collect::<Vec<_>>();
        let values = values
            .iter()
            .map(|x| x as _)
            .collect::<Vec<&(dyn ToSql + Sync)>>();
        let rows = self.db.query_all(&stmt, &values).await?;
        let mut buffer = Vec::with_capacity(rows.len());
        for row in rows.into_iter() {
            // TODO: Error handling
            buffer.push(S::from_raw_query_result(QueryResult { row })?);
        }
        Ok(buffer)
    }

    /// Fetch the current page
    // [spec:pgorm:sem:exec.paginator.fetch+1]
    pub async fn fetch(&self) -> Result<Vec<S::Item>, DbErr> {
        self.fetch_page(self.page).await
    }

    /// Get the total number of items
    // [spec:pgorm:sem:exec.paginator.count]
    pub async fn num_items(&self) -> Result<u64, DbErr> {
        let mut counted = self.query()?.clone();
        counted.reset_limit().reset_offset().clear_order_by();
        let stmt = SelectStatement::new()
            .expr(Expr::cust("COUNT(*) AS num_items"))
            .from_subquery(counted, Alias::new("sub_query"))
            .to_owned();
        let (stmt, values) = stmt.build();
        let values = values.into_iter().map(ValueHolder).collect::<Vec<_>>();
        let values = values
            .iter()
            .map(|x| x as _)
            .collect::<Vec<&(dyn ToSql + Sync)>>();
        let result = match self.db.query_opt(&stmt, &values).await? {
            Some(res) => res,
            None => return Ok(0),
        };
        let result = QueryResult { row: result };
        let num_items = result.try_get::<i64>("", "num_items")? as u64;
        Ok(num_items)
    }

    /// Get the total number of pages
    pub async fn num_pages(&self) -> Result<u64, DbErr> {
        let num_items = self.num_items().await?;
        let num_pages = self.compute_pages_number(num_items);
        Ok(num_pages)
    }

    /// Get the total number of items and pages
    pub async fn num_items_and_pages(&self) -> Result<ItemsAndPagesNumber, DbErr> {
        let number_of_items = self.num_items().await?;
        let number_of_pages = self.compute_pages_number(number_of_items);

        Ok(ItemsAndPagesNumber {
            number_of_items,
            number_of_pages,
        })
    }

    /// Compute the number of pages for the current page
    // [spec:pgorm:sem:exec.paginator.count]
    fn compute_pages_number(&self, num_items: u64) -> u64 {
        (num_items / self.page_size) + (num_items % self.page_size > 0) as u64
    }

    /// Increment the page counter
    pub fn next(&mut self) {
        self.page += 1;
    }

    /// Get current page number
    pub fn cur_page(&self) -> u64 {
        self.page
    }

    /// Fetch one page and increment the page counter
    ///
    /// Yields `None` once a page comes back empty.
    ///
    /// ```no_run
    /// # use std::num::NonZeroU64;
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool, PaginatorTrait};
    /// #
    /// # const PAGE_SIZE: NonZeroU64 = NonZeroU64::new(50).unwrap();
    /// # async fn example(pool: &DatabasePool) -> Result<(), DbErr> {
    /// let db = pool.get().await?;
    ///
    /// let mut cake_pages = cake::Entity::find()
    ///     .order_by_asc(cake::Column::Id)
    ///     .paginate(&db, PAGE_SIZE);
    ///
    /// while let Some(cakes) = cake_pages.fetch_and_next().await? {
    ///     // Do something on cakes: Vec<cake::Model>
    /// }
    /// # Ok(())
    /// # }
    /// ```
    // [spec:pgorm:sem:exec.paginator.iterate]
    pub async fn fetch_and_next(&mut self) -> Result<Option<Vec<S::Item>>, DbErr> {
        let vec = self.fetch().await?;
        self.next();
        let opt = if !vec.is_empty() { Some(vec) } else { None };
        Ok(opt)
    }

    /// Convert self into an async stream
    ///
    /// ```no_run
    /// # use std::num::NonZeroU64;
    /// # use futures::TryStreamExt;
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool, PaginatorTrait};
    /// #
    /// # const PAGE_SIZE: NonZeroU64 = NonZeroU64::new(50).unwrap();
    /// # async fn example(pool: &DatabasePool) -> Result<(), DbErr> {
    /// let db = pool.get().await?;
    ///
    /// let mut cake_stream = cake::Entity::find()
    ///     .order_by_asc(cake::Column::Id)
    ///     .paginate(&db, PAGE_SIZE)
    ///     .into_stream();
    ///
    /// while let Some(cakes) = cake_stream.try_next().await? {
    ///     // Do something on cakes: Vec<cake::Model>
    /// }
    /// # Ok(())
    /// # }
    /// ```
    // [spec:pgorm:sem:exec.paginator.iterate]
    pub fn into_stream(mut self) -> PinBoxStream<'db, Result<Vec<S::Item>, DbErr>> {
        Box::pin(stream! {
            while let Some(vec) = self.fetch_and_next().await? {
                yield Ok(vec);
            }
        })
    }
}

#[async_trait::async_trait]
/// A Trait for any type that can paginate results
// [spec:pgorm:def:exec.paginator+1]
pub trait PaginatorTrait<'db, C>
where
    C: ConnectionTrait,
{
    /// Select operation
    type Selector: SelectorTrait + Send + Sync + 'db;

    /// Paginate the result of a select operation.
    ///
    /// A zero page size — which would make every page empty and leave the
    /// page count undefined — is not a value this can be called with:
    ///
    /// ```compile_fail,E0308
    /// # use pgorm::{entity::prelude::*, tests_cfg::cake, DatabaseConnection, PaginatorTrait};
    /// # fn example(db: &DatabaseConnection) {
    /// cake::Entity::find().paginate(db, 0);
    /// # }
    /// ```
    ///
    /// A page size known at compile time is checked there, once:
    ///
    /// ```
    /// # use std::num::NonZeroU64;
    /// # use pgorm::{entity::prelude::*, tests_cfg::cake, DatabaseConnection, PaginatorTrait};
    /// const PAGE_SIZE: NonZeroU64 = NonZeroU64::new(50).unwrap();
    ///
    /// # fn example(db: &DatabaseConnection) {
    /// cake::Entity::find().paginate(db, PAGE_SIZE);
    /// # }
    /// ```
    // [spec:pgorm:req:exec.paginator.page-size+1/test]
    fn paginate(self, db: &'db C, page_size: NonZeroU64) -> Paginator<'db, C, Self::Selector>;

    /// Perform a count on the paginated results
    async fn count(self, db: &'db C) -> Result<u64, DbErr>
    where
        Self: Send + Sized,
    {
        self.paginate(db, NonZeroU64::MIN).num_items().await
    }
}

impl<'db, C, S> PaginatorTrait<'db, C> for Selector<S>
where
    C: ConnectionTrait,
    S: SelectorTrait + Send + Sync + 'db,
{
    type Selector = S;

    // [spec:pgorm:req:exec.paginator.page-size+1]
    fn paginate(self, db: &'db C, page_size: NonZeroU64) -> Paginator<'db, C, S> {
        Paginator {
            query: Ok(self.query),
            page: 0,
            page_size,
            db,
            selector: PhantomData,
        }
    }
}

impl<'db, C, S> PaginatorTrait<'db, C> for SelectorRaw<S>
where
    C: ConnectionTrait,
    S: SelectorTrait + Send + Sync + 'db,
{
    type Selector = S;
    // [spec:pgorm:req:exec.paginator.page-size+1]
    // [spec:pgorm:sem:exec.paginator.raw+1]
    fn paginate(self, db: &'db C, page_size: NonZeroU64) -> Paginator<'db, C, S> {
        Paginator {
            query: wrap_raw_select(&self.stmt, self.values.0),
            page: 0,
            page_size,
            db,
            selector: PhantomData,
        }
    }
}

/// The alias the caller's own statement is paged over as.
const RAW_SUBQUERY_ALIAS: &str = "sub_statement";

/// Wrap a caller's raw statement as `SELECT * FROM (<statement>) AS "sub_statement"`,
/// the shape `LIMIT` and `OFFSET` append to whatever clauses the statement carries
/// of its own, or report why it cannot be paged over at all.
// [spec:pgorm:sem:exec.paginator.raw+1]
fn wrap_raw_select(stmt: &str, values: Vec<Value>) -> Result<SelectStatement, String> {
    let select = single_select(stmt)?;
    let sql = format!(r#"* FROM ({select}) AS "{RAW_SUBQUERY_ALIAS}""#);

    let mut query = SelectStatement::new();
    query.expr(if values.is_empty() {
        Expr::cust(sql)
    } else {
        Expr::cust_with_values(sql, values)
    });
    Ok(query)
}

/// The one row-returning `SELECT` in `stmt`, at the extent libpg_query reports
/// for it — which excludes any terminating `;` a subquery position would refuse.
///
/// A `WITH ... SELECT` qualifies: PostgreSQL hangs the `WITH` clause off the
/// `SelectStmt` itself rather than making it a statement of its own.
// [spec:pgorm:sem:exec.paginator.raw+1]
fn single_select(stmt: &str) -> Result<&str, String> {
    let parsed = pg_query::parse(stmt).map_err(|error| {
        format!(
            "cannot paginate a raw statement PostgreSQL rejects: {}",
            parser_message(&error)
        )
    })?;

    let [raw] = parsed.protobuf.stmts.as_slice() else {
        return Err(format!(
            "cannot paginate a raw statement holding {} statements; pagination needs exactly one SELECT",
            parsed.protobuf.stmts.len()
        ));
    };

    let node = raw
        .stmt
        .as_ref()
        .and_then(|node| node.node.as_ref())
        .ok_or_else(|| "cannot paginate a raw statement that parsed to nothing".to_owned())?;

    match node {
        NodeEnum::SelectStmt(select) if select.into_clause.is_some() => Err(
            "cannot paginate a raw SELECT ... INTO; it creates a table rather than returning rows"
                .to_owned(),
        ),
        NodeEnum::SelectStmt(_) => extent(stmt, raw),
        other => Err(format!(
            "cannot paginate a raw statement that parses as {}; pagination needs a SELECT",
            node_name(other)
        )),
    }
}

/// The slice of `sql` that `raw` covers. `stmt_len` is zero for a statement
/// running to the end of the input with nothing terminating it.
fn extent<'sql>(sql: &'sql str, raw: &RawStmt) -> Result<&'sql str, String> {
    let start = usize::try_from(raw.stmt_location).unwrap_or(0);
    let end = match usize::try_from(raw.stmt_len) {
        Ok(0) | Err(_) => sql.len(),
        Ok(len) => start.saturating_add(len),
    };

    sql.get(start..end)
        .ok_or_else(|| "cannot paginate a raw statement PostgreSQL located outside it".to_owned())
}

/// PostgreSQL's own name for a parse node — `InsertStmt`, `VariableSetStmt` —
/// read off the head of its `Debug` form without rendering the tree beneath it.
fn node_name(node: &NodeEnum) -> String {
    let mut name = Head(String::new());
    let _ = write!(name, "{node:?}");
    name.0
}

/// A sink that keeps what a formatter writes before the first `(` and then stops
/// it, so a node's name costs nothing but the name.
struct Head(String);

impl fmt::Write for Head {
    fn write_str(&mut self, chunk: &str) -> fmt::Result {
        match chunk.split_once('(') {
            Some((head, _)) => {
                self.0.push_str(head);
                Err(fmt::Error)
            }
            None => {
                self.0.push_str(chunk);
                Ok(())
            }
        }
    }
}

/// The parser's own diagnostic, unwrapped from the `pg_query` error variant that
/// would otherwise prefix it with a stage name.
fn parser_message(error: &pg_query::Error) -> String {
    match error {
        pg_query::Error::Parse(message) => message.clone(),
        other => other.to_string(),
    }
}

impl<'db, C, M, E> PaginatorTrait<'db, C> for Select<E>
where
    C: ConnectionTrait,
    E: EntityTrait<Model = M>,
    M: FromQueryResult + Sized + Send + Sync + 'db,
{
    type Selector = SelectModel<M>;

    fn paginate(self, db: &'db C, page_size: NonZeroU64) -> Paginator<'db, C, Self::Selector> {
        self.into_model().paginate(db, page_size)
    }
}

impl<'db, C, M, N, E, F> PaginatorTrait<'db, C> for SelectTwo<E, F>
where
    C: ConnectionTrait,
    E: EntityTrait<Model = M>,
    F: EntityTrait<Model = N>,
    M: FromQueryResult + Sized + Send + Sync + 'db,
    N: FromQueryResult + Sized + Send + Sync + 'db,
{
    type Selector = SelectTwoModel<M, N>;

    fn paginate(self, db: &'db C, page_size: NonZeroU64) -> Paginator<'db, C, Self::Selector> {
        self.into_model().paginate(db, page_size)
    }
}
