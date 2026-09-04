use crate::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityName, EntityTrait, Insert,
    IntoActiveModel, Iterable, PrimaryKeyToColumn, PrimaryKeyTrait, QueryResult, SelectModel,
    SelectorRaw, TryInsert, error::*,
};
use pgorm_query::{Iden, InsertStatement, Query, TryFromValueTuple, ValueTuple};
use tokio_postgres::types::ToSql;

use super::ValueHolder;

/// The primary key an insert reports back, typed as the entity's declared
/// `PrimaryKey::ValueType`.
pub type InsertedPrimaryKey<A> =
    <<<A as ActiveModelTrait>::Entity as EntityTrait>::PrimaryKey as PrimaryKeyTrait>::ValueType;

/// The types of results for an INSERT operation
// [spec:pgorm:sem:exec.crud.try-insert+3]
#[derive(Debug)]
pub enum TryInsertResult<T> {
    /// The INSERT statement did not have any value to insert
    Empty,
    /// The INSERT operation did not insert any valid value
    Conflicted,
    /// Successfully inserted
    Inserted(T),
}

// [spec:pgorm:sem:exec.crud.try-insert+3]
impl<A> TryInsert<A>
where
    A: ActiveModelTrait,
{
    /// Whether the statement carries an `ON CONFLICT` clause, and so could have
    /// skipped the insert rather than failed it.
    fn has_conflict_clause(&self) -> bool {
        self.insert_struct.query.get_on_conflict().is_some()
    }

    /// Execute the insert and report how many rows it wrote.
    ///
    /// No `RETURNING` clause is emitted. See [`Self::exec_returning_pk`] for the
    /// inserted primary key and [`Self::exec_returning_model`] for the row.
    // [spec:pgorm:req:query.build.insert.uniform-columns+3]
    // [spec:pgorm:sem:exec.crud.exec-vocabulary]
    pub async fn exec<C>(self, db: &C) -> Result<TryInsertResult<u64>, Error>
    where
        C: ConnectionTrait,
    {
        self.ensure_uniform_columns()?;
        if self.insert_struct.is_empty() {
            return Ok(TryInsertResult::Empty);
        }
        let conflict_clause = self.has_conflict_clause();
        let res = self.insert_struct.exec(db).await;
        match res {
            Ok(0) if conflict_clause => Ok(TryInsertResult::Conflicted),
            Ok(res) => Ok(TryInsertResult::Inserted(res)),
            Err(Error::RecordNotInserted) => Ok(TryInsertResult::Conflicted),
            Err(err) => Err(err),
        }
    }

    /// Execute the insert and return the inserted row's primary key.
    // [spec:pgorm:req:query.build.insert.uniform-columns+3]
    // [spec:pgorm:sem:exec.crud.exec-vocabulary]
    pub async fn exec_returning_pk<C>(
        self,
        db: &C,
    ) -> Result<TryInsertResult<InsertedPrimaryKey<A>>, Error>
    where
        C: ConnectionTrait,
    {
        self.ensure_uniform_columns()?;
        if self.insert_struct.is_empty() {
            return Ok(TryInsertResult::Empty);
        }
        let res = self.insert_struct.exec_returning_pk(db).await;
        match res {
            Ok(res) => Ok(TryInsertResult::Inserted(res)),
            Err(Error::RecordNotInserted) => Ok(TryInsertResult::Conflicted),
            Err(err) => Err(err),
        }
    }

    /// Execute the insert and return the inserted row as a model.
    // [spec:pgorm:req:query.build.insert.uniform-columns+3]
    // [spec:pgorm:sem:exec.crud.exec-vocabulary]
    pub async fn exec_returning_model<C>(
        self,
        db: &C,
    ) -> Result<TryInsertResult<<A::Entity as EntityTrait>::Model>, Error>
    where
        <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
        C: ConnectionTrait,
    {
        self.ensure_uniform_columns()?;
        if self.insert_struct.is_empty() {
            return Ok(TryInsertResult::Empty);
        }
        let conflict_clause = self.has_conflict_clause();
        let res = exec_insert_returning_model_opt::<A, C>(self.insert_struct.query, db).await;
        match res {
            Ok(Some(res)) => Ok(TryInsertResult::Inserted(res)),
            Ok(None) if conflict_clause => Ok(TryInsertResult::Conflicted),
            Ok(None) => Err(Error::RecordNotFound),
            Err(err) => Err(err),
        }
    }
}

impl<A> Insert<A>
where
    A: ActiveModelTrait,
{
    /// Execute the insert and report how many rows it wrote.
    ///
    /// No `RETURNING` clause is emitted. See [`Self::exec_returning_pk`] for the
    /// inserted primary key and [`Self::exec_returning_model`] for the row.
    // [spec:pgorm:sem:exec.crud.exec-vocabulary]
    // [spec:pgorm:req:query.build.insert.uniform-columns+3]
    pub async fn exec<C>(self, db: &C) -> Result<u64, Error>
    where
        C: ConnectionTrait,
    {
        self.ensure_uniform_columns()?;
        exec_insert_without_returning(self.query, db).await
    }

    /// Execute the insert and return the inserted row's primary key.
    // [spec:pgorm:sem:exec.crud.insert+3]
    // [spec:pgorm:sem:exec.crud.exec-vocabulary]
    // [spec:pgorm:req:query.build.insert.uniform-columns+3]
    pub async fn exec_returning_pk<C>(self, db: &C) -> Result<InsertedPrimaryKey<A>, Error>
    where
        C: ConnectionTrait,
    {
        self.ensure_uniform_columns()?;
        let mut query = self.query;
        let returning =
            Query::returning().exprs(<A::Entity as EntityTrait>::PrimaryKey::iter().map(|c| {
                c.into_column()
                    .select_as(c.into_column().into_returning_expr())
            }));
        query.returning(returning);
        exec_insert_returning_pk::<A, _>(self.primary_key, query, db).await
    }

    /// Execute the insert and return the inserted row as a model.
    // [spec:pgorm:sem:exec.crud.insert-returning+2]
    // [spec:pgorm:sem:exec.crud.exec-vocabulary]
    // [spec:pgorm:req:query.build.insert.uniform-columns+3]
    pub async fn exec_returning_model<C>(
        self,
        db: &C,
    ) -> Result<<A::Entity as EntityTrait>::Model, Error>
    where
        <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
        C: ConnectionTrait,
    {
        self.ensure_uniform_columns()?;
        exec_insert_returning_model_opt::<A, _>(self.query, db)
            .await?
            .ok_or(Error::RecordNotFound)
    }
}

// [spec:pgorm:sem:exec.crud.insert+3]
async fn exec_insert_returning_pk<A, C>(
    primary_key: Option<ValueTuple>,
    statement: InsertStatement,
    db: &C,
) -> Result<InsertedPrimaryKey<A>, Error>
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

    match primary_key {
        Some(value_tuple) => {
            let res = db.execute(&stmt, &values).await?;
            if res == 0 {
                return Err(Error::RecordNotInserted);
            }
            TryFromValueTuple::try_from_value_tuple(value_tuple).map_err(|err| {
                primary_key_type_err(<A::Entity as Default>::default().table_name(), err)
            })
        }
        None => {
            let mut rows = db.query_all(&stmt, &values).await?;
            let row = match rows.pop() {
                Some(row) => QueryResult { row },
                None => return Err(Error::RecordNotInserted),
            };
            let cols = PrimaryKey::<A>::iter()
                .map(|col| col.to_string())
                .collect::<Vec<_>>();
            row.try_get_many("", cols.as_ref())
                .map_err(|_| Error::UnpackInsertId)
        }
    }
}

// [spec:pgorm:sem:exec.crud.insert-returning+2]
async fn exec_insert_without_returning<C>(
    insert_statement: InsertStatement,
    db: &C,
) -> Result<u64, Error>
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

/// A missing `RETURNING` row is reported as `None` rather than an error, so
/// callers that can tell an `ON CONFLICT` skip from a genuine miss decide which
/// it was.
// [spec:pgorm:sem:exec.crud.insert-returning+2]
async fn exec_insert_returning_model_opt<A, C>(
    mut insert_statement: InsertStatement,
    db: &C,
) -> Result<Option<<A::Entity as EntityTrait>::Model>, Error>
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
