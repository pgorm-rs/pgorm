use crate::{
    ConnectionTrait, EntityTrait, FromQueryResult, Select, SelectModel, SelectTwo, SelectTwoModel,
    Selector, SelectorRaw, SelectorTrait, error::*,
};
use async_stream::stream;
use futures::Stream;
use pgorm_query::{Alias, Expr, QueryBuilder, SelectStatement};
use std::{marker::PhantomData, pin::Pin};
use tokio_postgres::types::ToSql;

use super::{QueryResult, ValueHolder};

/// Pin a Model so that stream operations can be performed on the model
pub type PinBoxStream<'db, Item> = Pin<Box<dyn Stream<Item = Item> + 'db>>;

/// Defined a structure to handle pagination of a result from a query operation on a Model
// [spec:pgorm:def:exec.paginator]
#[derive(Clone, Debug)]
pub struct Paginator<'db, C, S>
where
    C: ConnectionTrait,
    S: SelectorTrait + 'db,
{
    pub(crate) query: SelectStatement,
    pub(crate) page: u64,
    pub(crate) page_size: u64,
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
    /// Fetch a specific page; page index starts from zero
    // [spec:pgorm:sem:exec.paginator.fetch]
    pub async fn fetch_page(&self, page: u64) -> Result<Vec<S::Item>, DbErr> {
        let query = self
            .query
            .clone()
            .limit(self.page_size)
            .offset(self.page_size * page)
            .to_owned();
        let (stmt, values) = query.build(QueryBuilder);
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
    // [spec:pgorm:sem:exec.paginator.fetch]
    pub async fn fetch(&self) -> Result<Vec<S::Item>, DbErr> {
        self.fetch_page(self.page).await
    }

    /// Get the total number of items
    // [spec:pgorm:sem:exec.paginator.count]
    pub async fn num_items(&self) -> Result<u64, DbErr> {
        let stmt = SelectStatement::new()
            .expr(Expr::cust("COUNT(*) AS num_items"))
            .from_subquery(
                self.query
                    .clone()
                    .reset_limit()
                    .reset_offset()
                    .clear_order_by()
                    .to_owned(),
                Alias::new("sub_query"),
            )
            .to_owned();
        let (stmt, values) = stmt.build(QueryBuilder);
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
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool, PaginatorTrait};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), DbErr> {
    /// let db = pool.get().await?;
    ///
    /// let mut cake_pages = cake::Entity::find()
    ///     .order_by_asc(cake::Column::Id)
    ///     .paginate(&db, 50);
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
    /// # use futures::TryStreamExt;
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool, PaginatorTrait};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), DbErr> {
    /// let db = pool.get().await?;
    ///
    /// let mut cake_stream = cake::Entity::find()
    ///     .order_by_asc(cake::Column::Id)
    ///     .paginate(&db, 50)
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
// [spec:pgorm:def:exec.paginator]
pub trait PaginatorTrait<'db, C>
where
    C: ConnectionTrait,
{
    /// Select operation
    type Selector: SelectorTrait + Send + Sync + 'db;

    /// Paginate the result of a select operation.
    fn paginate(self, db: &'db C, page_size: u64) -> Paginator<'db, C, Self::Selector>;

    /// Perform a count on the paginated results
    async fn count(self, db: &'db C) -> Result<u64, DbErr>
    where
        Self: Send + Sized,
    {
        self.paginate(db, 1).num_items().await
    }
}

impl<'db, C, S> PaginatorTrait<'db, C> for Selector<S>
where
    C: ConnectionTrait,
    S: SelectorTrait + Send + Sync + 'db,
{
    type Selector = S;

    // [spec:pgorm:req:exec.paginator.page-size]
    fn paginate(self, db: &'db C, page_size: u64) -> Paginator<'db, C, S> {
        assert!(page_size != 0, "page_size should not be zero");
        Paginator {
            query: self.query,
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
    // [spec:pgorm:req:exec.paginator.page-size]
    // [spec:pgorm:sem:exec.paginator.raw]
    fn paginate(self, db: &'db C, page_size: u64) -> Paginator<'db, C, S> {
        assert!(page_size != 0, "page_size should not be zero");
        let sql = self.stmt.trim()[6..].trim();
        let mut query = SelectStatement::new();
        query.expr(if !self.values.0.is_empty() {
            Expr::cust_with_values(sql, self.values.0)
        } else {
            Expr::cust(sql)
        });

        Paginator {
            query,
            page: 0,
            page_size,
            db,
            selector: PhantomData,
        }
    }
}

impl<'db, C, M, E> PaginatorTrait<'db, C> for Select<E>
where
    C: ConnectionTrait,
    E: EntityTrait<Model = M>,
    M: FromQueryResult + Sized + Send + Sync + 'db,
{
    type Selector = SelectModel<M>;

    fn paginate(self, db: &'db C, page_size: u64) -> Paginator<'db, C, Self::Selector> {
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

    fn paginate(self, db: &'db C, page_size: u64) -> Paginator<'db, C, Self::Selector> {
        self.into_model().paginate(db, page_size)
    }
}
