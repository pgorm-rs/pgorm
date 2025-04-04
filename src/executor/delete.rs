use crate::{ActiveModelTrait, ConnectionTrait, DeleteMany, DeleteOne, EntityTrait, error::*};
use pgorm_query::{DeleteStatement, QueryBuilder};
use std::future::Future;
use tokio_postgres::types::ToSql;

use super::ValueHolder;

/// Handles DELETE operations in a ActiveModel using [DeleteStatement]
#[derive(Clone, Debug)]
pub struct Deleter {
    query: DeleteStatement,
}

/// The result of a DELETE operation
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteResult {
    /// The number of rows affected by the DELETE operation
    pub rows_affected: u64,
}

impl<'a, A: 'a> DeleteOne<A>
where
    A: ActiveModelTrait,
{
    /// Execute a DELETE operation on one ActiveModel
    pub fn exec<C>(self, db: &'a C) -> impl Future<Output = Result<DeleteResult, DbErr>> + '_
    where
        C: ConnectionTrait,
    {
        // so that self is dropped before entering await
        exec_delete_only(self.query, db)
    }
}

impl<'a, E> DeleteMany<E>
where
    E: EntityTrait,
{
    /// Execute a DELETE operation on many ActiveModels
    pub fn exec<C>(self, db: &'a C) -> impl Future<Output = Result<DeleteResult, DbErr>> + '_
    where
        C: ConnectionTrait,
    {
        // so that self is dropped before entering await
        exec_delete_only(self.query, db)
    }
}

impl Deleter {
    /// Instantiate a new [Deleter] by passing it a [DeleteStatement]
    pub fn new(query: DeleteStatement) -> Self {
        Self { query }
    }

    /// Execute a DELETE operation
    pub fn exec<C>(self, db: &C) -> impl Future<Output = Result<DeleteResult, DbErr>> + '_
    where
        C: ConnectionTrait,
    {
        exec_delete(self.query, db)
    }
}

async fn exec_delete_only<C>(query: DeleteStatement, db: &C) -> Result<DeleteResult, DbErr>
where
    C: ConnectionTrait,
{
    Deleter::new(query).exec(db).await
}

async fn exec_delete<C>(query: DeleteStatement, db: &C) -> Result<DeleteResult, DbErr>
where
    C: ConnectionTrait,
{
    let (stmt, values) = query.build(QueryBuilder);
    let values = values.into_iter().map(ValueHolder).collect::<Vec<_>>();
    let values = values
        .iter()
        .map(|x| &*x as _)
        .collect::<Vec<&(dyn ToSql + Sync)>>();

    let result = db.execute(&stmt, &values).await?;
    Ok(DeleteResult {
        rows_affected: result,
    })
}
