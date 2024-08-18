use crate::{
    error::*, ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, Insert, IntoActiveModel, Iterable, PrimaryKeyToColumn, PrimaryKeyTrait, QueryResult, SelectModel, SelectorRaw, TryInsert
};
use sea_query::{FromValueTuple, Iden, InsertStatement, PostgresQueryBuilder, Query, ValueTuple};
use tokio_postgres::types::ToSql;
use std::{future::Future, marker::PhantomData};

use super::ValueHolder;

/// Defines a structure to perform INSERT operations in an ActiveModel
#[derive(Debug)]
pub struct Inserter<A>
where
    A: ActiveModelTrait,
{
    primary_key: Option<ValueTuple>,
    query: InsertStatement,
    model: PhantomData<A>,
}

/// The result of an INSERT operation on an ActiveModel
#[derive(Debug)]
pub struct InsertResult<A>
where
    A: ActiveModelTrait,
{
    /// The id performed when AUTOINCREMENT was performed on the PrimaryKey
    pub last_insert_id: <<<A as ActiveModelTrait>::Entity as EntityTrait>::PrimaryKey as PrimaryKeyTrait>::ValueType,
}

/// The types of results for an INSERT operation
#[derive(Debug)]
pub enum TryInsertResult<T> {
    /// The INSERT statement did not have any value to insert
    Empty,
    /// The INSERT operation did not insert any valid value
    Conflicted,
    /// Successfully inserted
    Inserted(T),
}

impl<A> TryInsert<A>
where
    A: ActiveModelTrait,
{
    /// Execute an insert operation
    #[allow(unused_mut)]
    pub async fn exec<'a, C>(self, db: &'a C) -> Result<TryInsertResult<InsertResult<A>>, DbErr>
    where
        C: ConnectionTrait,
        A: 'a,
    {
        if self.insert_struct.columns.is_empty() {
            return Ok(TryInsertResult::Empty);
        }
        let res = self.insert_struct.exec(db).await;
        match res {
            Ok(res) => Ok(TryInsertResult::Inserted(res)),
            Err(DbErr::RecordNotInserted) => Ok(TryInsertResult::Conflicted),
            Err(err) => Err(err),
        }
    }

    /// Execute an insert operation without returning (don't use `RETURNING` syntax)
    /// Number of rows affected is returned
    pub async fn exec_without_returning<'a, C>(
        self,
        db: &'a C,
    ) -> Result<TryInsertResult<u64>, DbErr>
    where
        <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
        C: ConnectionTrait,
        A: 'a,
    {
        if self.insert_struct.columns.is_empty() {
            return Ok(TryInsertResult::Empty);
        }
        let res = self.insert_struct.exec_without_returning(db).await;
        match res {
            Ok(res) => Ok(TryInsertResult::Inserted(res)),
            Err(DbErr::RecordNotInserted) => Ok(TryInsertResult::Conflicted),
            Err(err) => Err(err),
        }
    }

    /// Execute an insert operation and return the inserted model (use `RETURNING` syntax if supported)
    pub async fn exec_with_returning<'a, C>(
        self,
        db: &'a C,
    ) -> Result<TryInsertResult<<A::Entity as EntityTrait>::Model>, DbErr>
    where
        <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
        C: ConnectionTrait,
        A: 'a,
    {
        if self.insert_struct.columns.is_empty() {
            return Ok(TryInsertResult::Empty);
        }
        let res = self.insert_struct.exec_with_returning(db).await;
        match res {
            Ok(res) => Ok(TryInsertResult::Inserted(res)),
            Err(DbErr::RecordNotInserted) => Ok(TryInsertResult::Conflicted),
            Err(err) => Err(err),
        }
    }
}

impl<A> Insert<A>
where
    A: ActiveModelTrait,
{
    /// Execute an insert operation
    #[allow(unused_mut)]
    pub fn exec<'a, C>(self, db: &'a C) -> impl Future<Output = Result<InsertResult<A>, DbErr>> + '_
    where
        C: ConnectionTrait,
        A: 'a,
    {
        // so that self is dropped before entering await
        let mut query = self.query;
        let returning =
            Query::returning().exprs(<A::Entity as EntityTrait>::PrimaryKey::iter().map(|c| {
                c.into_column()
                    .select_as(c.into_column().into_returning_expr())
            }));
        query.returning(returning);
        Inserter::<A>::new(self.primary_key, query).exec(db)
    }

    /// Execute an insert operation without returning (don't use `RETURNING` syntax)
    /// Number of rows affected is returned
    pub fn exec_without_returning<'a, C>(
        self,
        db: &'a C,
    ) -> impl Future<Output = Result<u64, DbErr>> + '_
    where
        <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
        C: ConnectionTrait,
        A: 'a,
    {
        Inserter::<A>::new(self.primary_key, self.query).exec_without_returning(db)
    }

    /// Execute an insert operation and return the inserted model (use `RETURNING` syntax if supported)
    pub fn exec_with_returning<'a, C>(
        self,
        db: &'a C,
    ) -> impl Future<Output = Result<<A::Entity as EntityTrait>::Model, DbErr>> + '_
    where
        <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
        C: ConnectionTrait,
        A: 'a,
    {
        Inserter::<A>::new(self.primary_key, self.query).exec_with_returning(db)
    }
}

impl<A> Inserter<A>
where
    A: ActiveModelTrait,
{
    /// Instantiate a new insert operation
    pub fn new(primary_key: Option<ValueTuple>, query: InsertStatement) -> Self {
        Self {
            primary_key,
            query,
            model: PhantomData,
        }
    }

    /// Execute an insert operation, returning the last inserted id
    pub fn exec<'a, C>(self, db: &'a C) -> impl Future<Output = Result<InsertResult<A>, DbErr>> + '_
    where
        C: ConnectionTrait,
        A: 'a,
    {
        exec_insert(self.primary_key, self.query, db)
    }

    /// Execute an insert operation
    pub fn exec_without_returning<'a, C>(
        self,
        db: &'a C,
    ) -> impl Future<Output = Result<u64, DbErr>> + '_
    where
        C: ConnectionTrait,
        A: 'a,
    {
        exec_insert_without_returning(self.query, db)
    }

    /// Execute an insert operation and return the inserted model (use `RETURNING` syntax if supported)
    pub fn exec_with_returning<'a, C>(
        self,
        db: &'a C,
    ) -> impl Future<Output = Result<<A::Entity as EntityTrait>::Model, DbErr>> + '_
    where
        <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
        C: ConnectionTrait,
        A: 'a,
    {
        exec_insert_with_returning::<A, _>(self.primary_key, self.query, db)
    }
}

async fn exec_insert<A, C>(
    primary_key: Option<ValueTuple>,
    statement: InsertStatement,
    db: &C,
) -> Result<InsertResult<A>, DbErr>
where
    C: ConnectionTrait,
    A: ActiveModelTrait,
{
    let (stmt, values) = statement.build(PostgresQueryBuilder);
    let values = values.into_iter().map(ValueHolder).collect::<Vec<_>>();
    let values = values.iter().map(|x| &*x as _).collect::<Vec<&(dyn ToSql + Sync)>>();

    type PrimaryKey<A> = <<A as ActiveModelTrait>::Entity as EntityTrait>::PrimaryKey;
    type ValueTypeOf<A> = <PrimaryKey<A> as PrimaryKeyTrait>::ValueType;

    let last_insert_id = match primary_key {
        Some(value_tuple) => {
            let res = db.execute(&stmt, &values).await?;
            if res == 0 {
                return Err(DbErr::RecordNotInserted);
            }
            FromValueTuple::from_value_tuple(value_tuple)
        }
        None => {
            let mut rows = db.query_all(&stmt, &values).await?;
            let row = match rows.pop() {
                Some(row) => QueryResult { row },
                None => return Err(DbErr::RecordNotInserted),
            };
            let cols = PrimaryKey::<A>::iter()
                .map(|col| col.to_string())
                .collect::<Vec<_>>();
            row.try_get_many("", cols.as_ref())
                .map_err(|_| DbErr::UnpackInsertId)?
        }
    };

    Ok(InsertResult { last_insert_id })
}

async fn exec_insert_without_returning<C>(
    insert_statement: InsertStatement,
    db: &C,
) -> Result<u64, DbErr>
where
    C: ConnectionTrait,
{
    let (stmt, values) = insert_statement.build(PostgresQueryBuilder);
    let values = values.into_iter().map(ValueHolder).collect::<Vec<_>>();
    let values = values.iter().map(|x| &*x as _).collect::<Vec<&(dyn ToSql + Sync)>>();

    let exec_result = db.execute(&stmt, &values).await?;
    Ok(exec_result)
}

async fn exec_insert_with_returning<A, C>(
    primary_key: Option<ValueTuple>,
    mut insert_statement: InsertStatement,
    db: &C,
) -> Result<<A::Entity as EntityTrait>::Model, DbErr>
where
    <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
    C: ConnectionTrait,
    A: ActiveModelTrait,
{
    let returning = Query::returning().exprs(
        <A::Entity as EntityTrait>::Column::iter().map(|c| c.select_as(c.into_returning_expr())),
    );
    insert_statement.returning(returning);
    let (stmt, values) = insert_statement.build(PostgresQueryBuilder);

    let found = SelectorRaw::<SelectModel<<A::Entity as EntityTrait>::Model>>::from_statement(
        stmt, values
    )
    .one(db)
    .await?;
    match found {
        Some(model) => Ok(model),
        None => Err(DbErr::RecordNotFound(
            "Failed to find inserted item".to_owned(),
        )),
    }
}
