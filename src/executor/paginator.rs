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
            .map(|x| &*x as _)
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
    pub async fn fetch(&self) -> Result<Vec<S::Item>, DbErr> {
        self.fetch_page(self.page).await
    }

    /// Get the total number of items
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
            .map(|x| &*x as _)
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
    /// ```
    /// # use pgorm::{error::*, tests_cfg::*, *};
    /// #
    /// # #[smol_potat::main]
    /// # #[cfg(feature = "mock")]
    /// # pub async fn main() -> Result<(), DbErr> {
    /// #
    /// # let owned_db = MockDatabase::new(DbBackend::Postgres)
    /// #     .append_query_results([
    /// #         vec![cake::Model {
    /// #             id: 1,
    /// #             name: "Cake".to_owned(),
    /// #         }],
    /// #         vec![],
    /// #     ])
    /// #     .into_connection();
    /// # let db = &owned_db;
    /// #
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    /// let mut cake_pages = cake::Entity::find()
    ///     .order_by_asc(cake::Column::Id)
    ///     .paginate(db, 50);
    ///
    /// while let Some(cakes) = cake_pages.fetch_and_next().await? {
    ///     // Do something on cakes: Vec<cake::Model>
    /// }
    /// #
    /// # Ok(())
    /// # }
    /// ```
    pub async fn fetch_and_next(&mut self) -> Result<Option<Vec<S::Item>>, DbErr> {
        let vec = self.fetch().await?;
        self.next();
        let opt = if !vec.is_empty() { Some(vec) } else { None };
        Ok(opt)
    }

    /// Convert self into an async stream
    ///
    /// ```
    /// # use pgorm::{error::*, tests_cfg::*, *};
    /// #
    /// # #[smol_potat::main]
    /// # #[cfg(feature = "mock")]
    /// # pub async fn main() -> Result<(), DbErr> {
    /// #
    /// # let owned_db = MockDatabase::new(DbBackend::Postgres)
    /// #     .append_query_results([
    /// #         vec![cake::Model {
    /// #             id: 1,
    /// #             name: "Cake".to_owned(),
    /// #         }],
    /// #         vec![],
    /// #     ])
    /// #     .into_connection();
    /// # let db = &owned_db;
    /// #
    /// use futures::TryStreamExt;
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    /// let mut cake_stream = cake::Entity::find()
    ///     .order_by_asc(cake::Column::Id)
    ///     .paginate(db, 50)
    ///     .into_stream();
    ///
    /// while let Some(cakes) = cake_stream.try_next().await? {
    ///     // Do something on cakes: Vec<cake::Model>
    /// }
    /// #
    /// # Ok(())
    /// # }
    /// ```
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

pub trait RawPaginatorTrait<'db, C>
where
    C: ConnectionTrait,
{
    type Selector: SelectorTrait + Send + Sync + 'db;
    fn paginate_raw(self, db: &'db C, page_size: u64) -> RawPaginator<'db, C, Self::Selector>;
}

impl<'db, C, M> RawPaginatorTrait<'db, C> for SelectorRaw<SelectModel<M>>
where
    C: ConnectionTrait,
    M: FromQueryResult + Send + Sync + 'db,
{
    type Selector = SelectModel<M>;

    fn paginate_raw(self, db: &'db C, page_size: u64) -> RawPaginator<'db, C, Self::Selector> {
        assert!(page_size != 0, "page_size should not be zero");
        RawPaginator::new(self.stmt, page_size, db)
    }
}

pub struct RawPaginator<'db, C, S>
where
    C: ConnectionTrait,
    S: SelectorTrait + 'db,
{
    stmt: String,
    page: u64,
    page_size: u64,
    db: &'db C,
    selector: PhantomData<S>,
}

impl<'db, C, S> RawPaginator<'db, C, S>
where
    C: ConnectionTrait,
    S: SelectorTrait + 'db,
{
    pub fn new(stmt: String, page_size: u64, db: &'db C) -> Self {
        Self {
            stmt,
            page: 0,
            page_size,
            db,
            selector: PhantomData,
        }
    }

    pub async fn fetch_page(&self, page: u64) -> Result<Vec<S::Item>, DbErr> {
        let sql = format!(
            "{} LIMIT {} OFFSET {}",
            self.stmt,
            self.page_size,
            self.page_size * page
        );

        let rows = self.db.query_all(&sql, &[]).await?;
        let mut buffer = Vec::with_capacity(rows.len());
        for row in rows {
            buffer.push(S::from_raw_query_result(QueryResult { row })?);
        }
        Ok(buffer)
    }

    pub async fn fetch(&self) -> Result<Vec<S::Item>, DbErr> {
        self.fetch_page(self.page).await
    }

    pub async fn num_items(&self) -> Result<u64, DbErr> {
        let sql = format!(
            "SELECT COUNT(*) AS num_items FROM ({}) AS sub_query",
            self.stmt
        );

        let result = match self.db.query_opt(&sql, &[]).await? {
            Some(res) => res,
            None => return Ok(0),
        };
        let result = QueryResult { row: result };
        let num_items = result.try_get::<i64>("", "num_items")? as u64;
        Ok(num_items)
    }

    pub async fn num_pages(&self) -> Result<u64, DbErr> {
        let num_items = self.num_items().await?;
        let num_pages = self.compute_pages_number(num_items);
        Ok(num_pages)
    }

    pub async fn num_items_and_pages(&self) -> Result<ItemsAndPagesNumber, DbErr> {
        let number_of_items = self.num_items().await?;
        let number_of_pages = self.compute_pages_number(number_of_items);
        Ok(ItemsAndPagesNumber {
            number_of_items,
            number_of_pages,
        })
    }

    fn compute_pages_number(&self, num_items: u64) -> u64 {
        (num_items / self.page_size) + (num_items % self.page_size > 0) as u64
    }

    pub fn next(&mut self) {
        self.page += 1;
    }

    pub fn cur_page(&self) -> u64 {
        self.page
    }

    pub async fn fetch_and_next(&mut self) -> Result<Option<Vec<S::Item>>, DbErr> {
        let vec = self.fetch().await?;
        self.next();
        let opt = if !vec.is_empty() { Some(vec) } else { None };
        Ok(opt)
    }

    pub fn into_stream(mut self) -> PinBoxStream<'db, Result<Vec<S::Item>, DbErr>> {
        Box::pin(stream! {
            while let Some(vec) = self.fetch_and_next().await? {
                yield Ok(vec);
            }
        })
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

#[cfg(test)]
#[cfg(feature = "mock")]
mod tests {
    use super::*;
    use crate::entity::prelude::*;
    use crate::{ConnectionTrait, Statement, tests_cfg::*};
    use crate::{DatabasePool, DbBackend, MockDatabase, Transaction};
    use futures::TryStreamExt;
    use once_cell::sync::Lazy;
    use pgorm_query::{Alias, Expr, SelectStatement, Value};
    use pretty_assertions::assert_eq;

    static RAW_STMT: Lazy<Statement> = Lazy::new(|| {
        Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"SELECT "fruit"."id", "fruit"."name", "fruit"."cake_id" FROM "fruit""#,
            [],
        )
    });

    fn setup() -> (DatabasePool, Vec<Vec<fruit::Model>>) {
        let page1 = vec![
            fruit::Model {
                id: 1,
                name: "Blueberry".into(),
                cake_id: Some(1),
            },
            fruit::Model {
                id: 2,
                name: "Rasberry".into(),
                cake_id: Some(1),
            },
        ];

        let page2 = vec![fruit::Model {
            id: 3,
            name: "Strawberry".into(),
            cake_id: Some(2),
        }];

        let page3 = Vec::<fruit::Model>::new();

        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([page1.clone(), page2.clone(), page3.clone()])
            .into_connection();

        (db, vec![page1, page2, page3])
    }

    fn setup_num_items() -> (DatabasePool, i64) {
        let num_items = 3;
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[maplit::btreemap! {
                "num_items" => Into::<Value>::into(num_items),
            }]])
            .into_connection();

        (db, num_items)
    }

    #[smol_potat::test]
    async fn fetch_page() -> Result<(), DbErr> {
        let (db, pages) = setup();

        let paginator = fruit::Entity::find().paginate(&db, 2);

        assert_eq!(paginator.fetch_page(0).await?, pages[0].clone());
        assert_eq!(paginator.fetch_page(1).await?, pages[1].clone());
        assert_eq!(paginator.fetch_page(2).await?, pages[2].clone());

        let mut select = SelectStatement::new()
            .exprs([
                Expr::col((fruit::Entity, fruit::Column::Id)),
                Expr::col((fruit::Entity, fruit::Column::Name)),
                Expr::col((fruit::Entity, fruit::Column::CakeId)),
            ])
            .from(fruit::Entity)
            .to_owned();

        let query_builder = db.get_database_backend();
        let stmts = [
            query_builder.build(select.clone().offset(0).limit(2)),
            query_builder.build(select.clone().offset(2).limit(2)),
            query_builder.build(select.offset(4).limit(2)),
        ];

        assert_eq!(db.into_transaction_log(), Transaction::wrap(stmts));
        Ok(())
    }

    #[smol_potat::test]
    async fn fetch_page_raw() -> Result<(), DbErr> {
        let (db, pages) = setup();

        let paginator = fruit::Entity::find()
            .from_raw_sql(RAW_STMT.clone())
            .paginate(&db, 2);

        assert_eq!(paginator.fetch_page(0).await?, pages[0].clone());
        assert_eq!(paginator.fetch_page(1).await?, pages[1].clone());
        assert_eq!(paginator.fetch_page(2).await?, pages[2].clone());

        let mut select = SelectStatement::new()
            .exprs([
                Expr::col((fruit::Entity, fruit::Column::Id)),
                Expr::col((fruit::Entity, fruit::Column::Name)),
                Expr::col((fruit::Entity, fruit::Column::CakeId)),
            ])
            .from(fruit::Entity)
            .to_owned();

        let query_builder = db.get_database_backend();
        let stmts = [
            query_builder.build(select.clone().offset(0).limit(2)),
            query_builder.build(select.clone().offset(2).limit(2)),
            query_builder.build(select.offset(4).limit(2)),
        ];

        assert_eq!(db.into_transaction_log(), Transaction::wrap(stmts));
        Ok(())
    }

    #[smol_potat::test]
    async fn fetch() -> Result<(), DbErr> {
        let (db, pages) = setup();

        let mut paginator = fruit::Entity::find().paginate(&db, 2);

        assert_eq!(paginator.fetch().await?, pages[0].clone());
        paginator.next();

        assert_eq!(paginator.fetch().await?, pages[1].clone());
        paginator.next();

        assert_eq!(paginator.fetch().await?, pages[2].clone());

        let mut select = SelectStatement::new()
            .exprs([
                Expr::col((fruit::Entity, fruit::Column::Id)),
                Expr::col((fruit::Entity, fruit::Column::Name)),
                Expr::col((fruit::Entity, fruit::Column::CakeId)),
            ])
            .from(fruit::Entity)
            .to_owned();

        let query_builder = db.get_database_backend();
        let stmts = [
            query_builder.build(select.clone().offset(0).limit(2)),
            query_builder.build(select.clone().offset(2).limit(2)),
            query_builder.build(select.offset(4).limit(2)),
        ];

        assert_eq!(db.into_transaction_log(), Transaction::wrap(stmts));
        Ok(())
    }

    #[smol_potat::test]
    async fn fetch_raw() -> Result<(), DbErr> {
        let (db, pages) = setup();

        let mut paginator = fruit::Entity::find()
            .from_raw_sql(RAW_STMT.clone())
            .paginate(&db, 2);

        assert_eq!(paginator.fetch().await?, pages[0].clone());
        paginator.next();

        assert_eq!(paginator.fetch().await?, pages[1].clone());
        paginator.next();

        assert_eq!(paginator.fetch().await?, pages[2].clone());

        let mut select = SelectStatement::new()
            .exprs([
                Expr::col((fruit::Entity, fruit::Column::Id)),
                Expr::col((fruit::Entity, fruit::Column::Name)),
                Expr::col((fruit::Entity, fruit::Column::CakeId)),
            ])
            .from(fruit::Entity)
            .to_owned();

        let query_builder = db.get_database_backend();
        let stmts = [
            query_builder.build(select.clone().offset(0).limit(2)),
            query_builder.build(select.clone().offset(2).limit(2)),
            query_builder.build(select.offset(4).limit(2)),
        ];

        assert_eq!(db.into_transaction_log(), Transaction::wrap(stmts));
        Ok(())
    }

    #[smol_potat::test]
    async fn num_pages() -> Result<(), DbErr> {
        let (db, num_items) = setup_num_items();

        let num_items = num_items as u64;
        let page_size = 2_u64;
        let num_pages = (num_items / page_size) + (num_items % page_size > 0) as u64;
        let paginator = fruit::Entity::find().paginate(&db, page_size);

        assert_eq!(paginator.num_pages().await?, num_pages);

        let sub_query = SelectStatement::new()
            .exprs([
                Expr::col((fruit::Entity, fruit::Column::Id)),
                Expr::col((fruit::Entity, fruit::Column::Name)),
                Expr::col((fruit::Entity, fruit::Column::CakeId)),
            ])
            .from(fruit::Entity)
            .to_owned();

        let select = SelectStatement::new()
            .expr(Expr::cust("COUNT(*) AS num_items"))
            .from_subquery(sub_query, Alias::new("sub_query"))
            .to_owned();

        let query_builder = db.get_database_backend();
        let stmts = [query_builder.build(&select)];

        assert_eq!(db.into_transaction_log(), Transaction::wrap(stmts));
        Ok(())
    }

    #[smol_potat::test]
    async fn num_pages_raw() -> Result<(), DbErr> {
        let (db, num_items) = setup_num_items();

        let num_items = num_items as u64;
        let page_size = 2_u64;
        let num_pages = (num_items / page_size) + (num_items % page_size > 0) as u64;
        let paginator = fruit::Entity::find()
            .from_raw_sql(RAW_STMT.clone())
            .paginate(&db, page_size);

        assert_eq!(paginator.num_pages().await?, num_pages);

        let sub_query = SelectStatement::new()
            .exprs([
                Expr::col((fruit::Entity, fruit::Column::Id)),
                Expr::col((fruit::Entity, fruit::Column::Name)),
                Expr::col((fruit::Entity, fruit::Column::CakeId)),
            ])
            .from(fruit::Entity)
            .to_owned();

        let select = SelectStatement::new()
            .expr(Expr::cust("COUNT(*) AS num_items"))
            .from_subquery(sub_query, Alias::new("sub_query"))
            .to_owned();

        let query_builder = db.get_database_backend();
        let stmts = [query_builder.build(&select)];

        assert_eq!(db.into_transaction_log(), Transaction::wrap(stmts));
        Ok(())
    }

    #[smol_potat::test]
    async fn next_and_cur_page() -> Result<(), DbErr> {
        let (db, _) = setup();

        let mut paginator = fruit::Entity::find().paginate(&db, 2);

        assert_eq!(paginator.cur_page(), 0);
        paginator.next();

        assert_eq!(paginator.cur_page(), 1);
        paginator.next();

        assert_eq!(paginator.cur_page(), 2);
        Ok(())
    }

    #[smol_potat::test]
    async fn next_and_cur_page_raw() -> Result<(), DbErr> {
        let (db, _) = setup();

        let mut paginator = fruit::Entity::find()
            .from_raw_sql(RAW_STMT.clone())
            .paginate(&db, 2);

        assert_eq!(paginator.cur_page(), 0);
        paginator.next();

        assert_eq!(paginator.cur_page(), 1);
        paginator.next();

        assert_eq!(paginator.cur_page(), 2);
        Ok(())
    }

    #[smol_potat::test]
    async fn fetch_and_next() -> Result<(), DbErr> {
        let (db, pages) = setup();

        let mut paginator = fruit::Entity::find().paginate(&db, 2);

        assert_eq!(paginator.cur_page(), 0);
        assert_eq!(paginator.fetch_and_next().await?, Some(pages[0].clone()));

        assert_eq!(paginator.cur_page(), 1);
        assert_eq!(paginator.fetch_and_next().await?, Some(pages[1].clone()));

        assert_eq!(paginator.cur_page(), 2);
        assert_eq!(paginator.fetch_and_next().await?, None);

        let mut select = SelectStatement::new()
            .exprs([
                Expr::col((fruit::Entity, fruit::Column::Id)),
                Expr::col((fruit::Entity, fruit::Column::Name)),
                Expr::col((fruit::Entity, fruit::Column::CakeId)),
            ])
            .from(fruit::Entity)
            .to_owned();

        let query_builder = db.get_database_backend();
        let stmts = [
            query_builder.build(select.clone().offset(0).limit(2)),
            query_builder.build(select.clone().offset(2).limit(2)),
            query_builder.build(select.offset(4).limit(2)),
        ];

        assert_eq!(db.into_transaction_log(), Transaction::wrap(stmts));
        Ok(())
    }

    #[smol_potat::test]
    async fn fetch_and_next_raw() -> Result<(), DbErr> {
        let (db, pages) = setup();

        let mut paginator = fruit::Entity::find()
            .from_raw_sql(RAW_STMT.clone())
            .paginate(&db, 2);

        assert_eq!(paginator.cur_page(), 0);
        assert_eq!(paginator.fetch_and_next().await?, Some(pages[0].clone()));

        assert_eq!(paginator.cur_page(), 1);
        assert_eq!(paginator.fetch_and_next().await?, Some(pages[1].clone()));

        assert_eq!(paginator.cur_page(), 2);
        assert_eq!(paginator.fetch_and_next().await?, None);

        let mut select = SelectStatement::new()
            .exprs([
                Expr::col((fruit::Entity, fruit::Column::Id)),
                Expr::col((fruit::Entity, fruit::Column::Name)),
                Expr::col((fruit::Entity, fruit::Column::CakeId)),
            ])
            .from(fruit::Entity)
            .to_owned();

        let query_builder = db.get_database_backend();
        let stmts = [
            query_builder.build(select.clone().offset(0).limit(2)),
            query_builder.build(select.clone().offset(2).limit(2)),
            query_builder.build(select.offset(4).limit(2)),
        ];

        assert_eq!(db.into_transaction_log(), Transaction::wrap(stmts));
        Ok(())
    }

    #[smol_potat::test]
    async fn into_stream() -> Result<(), DbErr> {
        let (db, pages) = setup();

        let mut fruit_stream = fruit::Entity::find().paginate(&db, 2).into_stream();

        assert_eq!(fruit_stream.try_next().await?, Some(pages[0].clone()));
        assert_eq!(fruit_stream.try_next().await?, Some(pages[1].clone()));
        assert_eq!(fruit_stream.try_next().await?, None);

        drop(fruit_stream);

        let mut select = SelectStatement::new()
            .exprs([
                Expr::col((fruit::Entity, fruit::Column::Id)),
                Expr::col((fruit::Entity, fruit::Column::Name)),
                Expr::col((fruit::Entity, fruit::Column::CakeId)),
            ])
            .from(fruit::Entity)
            .to_owned();

        let query_builder = db.get_database_backend();
        let stmts = [
            query_builder.build(select.clone().offset(0).limit(2)),
            query_builder.build(select.clone().offset(2).limit(2)),
            query_builder.build(select.offset(4).limit(2)),
        ];

        assert_eq!(db.into_transaction_log(), Transaction::wrap(stmts));
        Ok(())
    }

    #[smol_potat::test]
    async fn into_stream_raw() -> Result<(), DbErr> {
        let (db, pages) = setup();

        let mut fruit_stream = fruit::Entity::find()
            .from_raw_sql(RAW_STMT.clone())
            .paginate(&db, 2)
            .into_stream();

        assert_eq!(fruit_stream.try_next().await?, Some(pages[0].clone()));
        assert_eq!(fruit_stream.try_next().await?, Some(pages[1].clone()));
        assert_eq!(fruit_stream.try_next().await?, None);

        drop(fruit_stream);

        let mut select = SelectStatement::new()
            .exprs([
                Expr::col((fruit::Entity, fruit::Column::Id)),
                Expr::col((fruit::Entity, fruit::Column::Name)),
                Expr::col((fruit::Entity, fruit::Column::CakeId)),
            ])
            .from(fruit::Entity)
            .to_owned();

        let query_builder = db.get_database_backend();
        let stmts = [
            query_builder.build(select.clone().offset(0).limit(2)),
            query_builder.build(select.clone().offset(2).limit(2)),
            query_builder.build(select.offset(4).limit(2)),
        ];

        assert_eq!(db.into_transaction_log(), Transaction::wrap(stmts));
        Ok(())
    }

    #[smol_potat::test]
    async fn into_stream_raw_leading_spaces() -> Result<(), DbErr> {
        let (db, pages) = setup();

        let raw_stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"  SELECT "fruit"."id", "fruit"."name", "fruit"."cake_id" FROM "fruit"  "#,
            [],
        );

        let mut fruit_stream = fruit::Entity::find()
            .from_raw_sql(raw_stmt.clone())
            .paginate(&db, 2)
            .into_stream();

        assert_eq!(fruit_stream.try_next().await?, Some(pages[0].clone()));
        assert_eq!(fruit_stream.try_next().await?, Some(pages[1].clone()));
        assert_eq!(fruit_stream.try_next().await?, None);

        drop(fruit_stream);

        let mut select = SelectStatement::new()
            .exprs([
                Expr::col((fruit::Entity, fruit::Column::Id)),
                Expr::col((fruit::Entity, fruit::Column::Name)),
                Expr::col((fruit::Entity, fruit::Column::CakeId)),
            ])
            .from(fruit::Entity)
            .to_owned();

        let query_builder = db.get_database_backend();
        let stmts = [
            query_builder.build(select.clone().offset(0).limit(2)),
            query_builder.build(select.clone().offset(2).limit(2)),
            query_builder.build(select.offset(4).limit(2)),
        ];

        assert_eq!(db.into_transaction_log(), Transaction::wrap(stmts));
        Ok(())
    }

    #[smol_potat::test]
    #[should_panic]
    async fn error() {
        let (db, _pages) = setup();

        fruit::Entity::find().paginate(&db, 0);
    }
}
