use crate::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityName, EntityTrait, Insert,
    IntoActiveModel, Iterable, PrimaryKeyToColumn, PrimaryKeyTrait, QueryResult, SelectModel,
    SelectorRaw, TryInsert, error::*,
};
use pgorm_query::{Iden, InsertStatement, Query, TryFromValueTuple, ValueTuple};
use std::{future::Future, marker::PhantomData};
use tokio_postgres::types::ToSql;

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
// [spec:pgorm:sem:exec.crud.try-insert+1]
#[derive(Debug)]
pub enum TryInsertResult<T> {
    /// The INSERT statement did not have any value to insert
    Empty,
    /// The INSERT operation did not insert any valid value
    Conflicted,
    /// Successfully inserted
    Inserted(T),
}

// [spec:pgorm:sem:exec.crud.try-insert+1]
impl<A> TryInsert<A>
where
    A: ActiveModelTrait,
{
    /// Whether the statement carries an `ON CONFLICT` clause, and so could have
    /// skipped the insert rather than failed it.
    fn has_conflict_clause(&self) -> bool {
        self.insert_struct.query.get_on_conflict().is_some()
    }

    /// Execute an insert operation
    // [spec:pgorm:req:query.build.insert.uniform-columns+1]
    #[allow(unused_mut)]
    pub async fn exec<'a, C>(self, db: &'a C) -> Result<TryInsertResult<InsertResult<A>>, DbErr>
    where
        C: ConnectionTrait,
        A: 'a,
    {
        self.ensure_uniform_columns()?;
        if self.insert_struct.is_empty() {
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
    // [spec:pgorm:req:query.build.insert.uniform-columns+1]
    pub async fn exec_without_returning<'a, C>(
        self,
        db: &'a C,
    ) -> Result<TryInsertResult<u64>, DbErr>
    where
        <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
        C: ConnectionTrait,
        A: 'a,
    {
        self.ensure_uniform_columns()?;
        if self.insert_struct.is_empty() {
            return Ok(TryInsertResult::Empty);
        }
        let conflict_clause = self.has_conflict_clause();
        let res = self.insert_struct.exec_without_returning(db).await;
        match res {
            Ok(0) if conflict_clause => Ok(TryInsertResult::Conflicted),
            Ok(res) => Ok(TryInsertResult::Inserted(res)),
            Err(DbErr::RecordNotInserted) => Ok(TryInsertResult::Conflicted),
            Err(err) => Err(err),
        }
    }

    /// Execute an insert operation and return the inserted model (use `RETURNING` syntax if supported)
    // [spec:pgorm:req:query.build.insert.uniform-columns+1]
    pub async fn exec_with_returning<'a, C>(
        self,
        db: &'a C,
    ) -> Result<TryInsertResult<<A::Entity as EntityTrait>::Model>, DbErr>
    where
        <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
        C: ConnectionTrait,
        A: 'a,
    {
        self.ensure_uniform_columns()?;
        if self.insert_struct.is_empty() {
            return Ok(TryInsertResult::Empty);
        }
        let conflict_clause = self.has_conflict_clause();
        let res = exec_insert_with_returning_opt::<A, C>(self.insert_struct.query, db).await;
        match res {
            Ok(Some(res)) => Ok(TryInsertResult::Inserted(res)),
            Ok(None) if conflict_clause => Ok(TryInsertResult::Conflicted),
            Ok(None) => Err(DbErr::RecordNotFound),
            Err(err) => Err(err),
        }
    }
}

impl<A> Insert<A>
where
    A: ActiveModelTrait,
{
    /// Execute an insert operation
    // [spec:pgorm:sem:exec.crud.insert+1]
    // [spec:pgorm:req:query.build.insert.uniform-columns+1]
    #[allow(unused_mut)]
    pub fn exec<'a, C>(self, db: &'a C) -> impl Future<Output = Result<InsertResult<A>, DbErr>> + 'a
    where
        C: ConnectionTrait,
        A: 'a,
    {
        // so that self is dropped before entering await
        let inserter = self.ensure_uniform_columns().map(|()| {
            let mut query = self.query;
            let returning =
                Query::returning().exprs(<A::Entity as EntityTrait>::PrimaryKey::iter().map(|c| {
                    c.into_column()
                        .select_as(c.into_column().into_returning_expr())
                }));
            query.returning(returning);
            Inserter::<A>::new(self.primary_key, query)
        });
        async move { inserter?.exec(db).await }
    }

    /// Execute an insert operation without returning (don't use `RETURNING` syntax)
    /// Number of rows affected is returned
    // [spec:pgorm:req:query.build.insert.uniform-columns+1]
    pub fn exec_without_returning<'a, C>(
        self,
        db: &'a C,
    ) -> impl Future<Output = Result<u64, DbErr>> + 'a
    where
        <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
        C: ConnectionTrait,
        A: 'a,
    {
        let inserter = self
            .ensure_uniform_columns()
            .map(|()| Inserter::<A>::new(self.primary_key, self.query));
        async move { inserter?.exec_without_returning(db).await }
    }

    /// Execute an insert operation and return the inserted model (use `RETURNING` syntax if supported)
    // [spec:pgorm:req:query.build.insert.uniform-columns+1]
    pub fn exec_with_returning<'a, C>(
        self,
        db: &'a C,
    ) -> impl Future<Output = Result<<A::Entity as EntityTrait>::Model, DbErr>> + 'a
    where
        <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
        C: ConnectionTrait,
        A: 'a,
    {
        let inserter = self
            .ensure_uniform_columns()
            .map(|()| Inserter::<A>::new(self.primary_key, self.query));
        async move { inserter?.exec_with_returning(db).await }
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
    pub fn exec<'a, C>(self, db: &'a C) -> impl Future<Output = Result<InsertResult<A>, DbErr>> + 'a
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
    ) -> impl Future<Output = Result<u64, DbErr>> + 'a
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
    ) -> impl Future<Output = Result<<A::Entity as EntityTrait>::Model, DbErr>> + 'a
    where
        <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
        C: ConnectionTrait,
        A: 'a,
    {
        exec_insert_with_returning::<A, _>(self.query, db)
    }
}

// [spec:pgorm:sem:exec.crud.insert+1]
async fn exec_insert<A, C>(
    primary_key: Option<ValueTuple>,
    statement: InsertStatement,
    db: &C,
) -> Result<InsertResult<A>, DbErr>
where
    C: ConnectionTrait,
    A: ActiveModelTrait,
{
    let (stmt, values) = statement.build();
    let values = values.into_iter().map(ValueHolder).collect::<Vec<_>>();
    let values = values
        .iter()
        .map(|x| x as _)
        .collect::<Vec<&(dyn ToSql + Sync)>>();

    type PrimaryKey<A> = <<A as ActiveModelTrait>::Entity as EntityTrait>::PrimaryKey;

    let last_insert_id = match primary_key {
        Some(value_tuple) => {
            let res = db.execute(&stmt, &values).await?;
            if res == 0 {
                return Err(DbErr::RecordNotInserted);
            }
            TryFromValueTuple::try_from_value_tuple(value_tuple).map_err(|err| {
                primary_key_type_err(<A::Entity as Default>::default().table_name(), err)
            })?
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

// [spec:pgorm:sem:exec.crud.insert-returning]
async fn exec_insert_without_returning<C>(
    insert_statement: InsertStatement,
    db: &C,
) -> Result<u64, DbErr>
where
    C: ConnectionTrait,
{
    let (stmt, values) = insert_statement.build();
    let values = values.into_iter().map(ValueHolder).collect::<Vec<_>>();
    let values = values
        .iter()
        .map(|x| x as _)
        .collect::<Vec<&(dyn ToSql + Sync)>>();

    let exec_result = db.execute(&stmt, &values).await?;
    Ok(exec_result)
}

// [spec:pgorm:sem:exec.crud.insert-returning]
async fn exec_insert_with_returning<A, C>(
    insert_statement: InsertStatement,
    db: &C,
) -> Result<<A::Entity as EntityTrait>::Model, DbErr>
where
    <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
    C: ConnectionTrait,
    A: ActiveModelTrait,
{
    exec_insert_with_returning_opt::<A, C>(insert_statement, db)
        .await?
        .ok_or(DbErr::RecordNotFound)
}

/// A missing `RETURNING` row is reported as `None` rather than an error, so
/// callers that can tell an `ON CONFLICT` skip from a genuine miss decide which
/// it was.
async fn exec_insert_with_returning_opt<A, C>(
    mut insert_statement: InsertStatement,
    db: &C,
) -> Result<Option<<A::Entity as EntityTrait>::Model>, DbErr>
where
    <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
    C: ConnectionTrait,
    A: ActiveModelTrait,
{
    let returning = Query::returning().exprs(
        <A::Entity as EntityTrait>::Column::iter().map(|c| c.select_as(c.into_returning_expr())),
    );
    insert_statement.returning(returning);
    let (stmt, values) = insert_statement.build();

    SelectorRaw::<SelectModel<<A::Entity as EntityTrait>::Model>>::from_statement(stmt, values)
        .one_opt(db)
        .await
}
