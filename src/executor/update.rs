use crate::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, Iterable,
    PrimaryKeyTrait, SelectModel, SelectorRaw, UpdateMany, UpdateOne, error::*,
};
use pgorm_query::{FromValueTuple, Query, QueryBuilder, UpdateStatement};
use tokio_postgres::types::ToSql;

use super::ValueHolder;

/// Defines an update operation
#[derive(Clone, Debug)]
pub struct Updater {
    query: UpdateStatement,
    check_record_exists: bool,
}

/// The result of an update operation on an ActiveModel
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct UpdateResult {
    /// The rows affected by the update operation
    pub rows_affected: u64,
}

impl<'a, A: 'a> UpdateOne<A>
where
    A: ActiveModelTrait,
{
    /// Execute an update operation on an ActiveModel
    pub async fn exec<'b, C>(self, db: &'b C) -> Result<<A::Entity as EntityTrait>::Model, DbErr>
    where
        <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
        C: ConnectionTrait,
    {
        Updater::new(self.query)
            .exec_update_and_return_updated(self.model, db)
            .await
    }
}

impl<'a, E> UpdateMany<E>
where
    E: EntityTrait,
{
    /// Execute an update operation on multiple ActiveModels
    pub async fn exec<C>(self, db: &'a C) -> Result<UpdateResult, DbErr>
    where
        C: ConnectionTrait,
    {
        Updater::new(self.query).exec(db).await
    }

    /// Execute an update operation and return the updated model (use `RETURNING` syntax if supported)
    ///
    /// # Panics
    ///
    /// Panics if the database backend does not support `UPDATE RETURNING`.
    pub async fn exec_with_returning<C>(self, db: &'a C) -> Result<Vec<E::Model>, DbErr>
    where
        C: ConnectionTrait,
    {
        Updater::new(self.query)
            .exec_update_with_returning::<E, _>(db)
            .await
    }
}

// [spec:pgorm:sem:exec.crud.update]
impl Updater {
    /// Instantiate an update using an [UpdateStatement]
    pub fn new(query: UpdateStatement) -> Self {
        Self {
            query,
            check_record_exists: false,
        }
    }

    /// Check if a record exists on the ActiveModel to perform the update operation on
    pub fn check_record_exists(mut self) -> Self {
        self.check_record_exists = true;
        self
    }

    /// Execute an update operation
    pub async fn exec<C>(self, db: &C) -> Result<UpdateResult, DbErr>
    where
        C: ConnectionTrait,
    {
        if self.is_noop() {
            return Ok(UpdateResult::default());
        }
        let (stmt, values) = self.query.build(QueryBuilder);
        let values = values.into_iter().map(ValueHolder).collect::<Vec<_>>();
        let values = values
            .iter()
            .map(|x| &*x as _)
            .collect::<Vec<&(dyn ToSql + Sync)>>();

        let result = db.execute(&stmt, &values).await?;
        if self.check_record_exists && result == 0 {
            return Err(DbErr::RecordNotUpdated);
        }
        Ok(UpdateResult {
            rows_affected: result,
        })
    }

    async fn exec_update_and_return_updated<A, C>(
        mut self,
        model: A,
        db: &C,
    ) -> Result<<A::Entity as EntityTrait>::Model, DbErr>
    where
        A: ActiveModelTrait,
        C: ConnectionTrait,
    {
        type Entity<A> = <A as ActiveModelTrait>::Entity;
        type Model<A> = <Entity<A> as EntityTrait>::Model;
        type Column<A> = <Entity<A> as EntityTrait>::Column;

        if self.is_noop() {
            return find_updated_model_by_id(model, db).await;
        }

        let returning = Query::returning()
            .exprs(Column::<A>::iter().map(|c| c.select_as(c.into_returning_expr())));
        self.query.returning(returning);

        let (stmt, values) = self.query.build(QueryBuilder);

        let found: Model<A> = SelectorRaw::<SelectModel<Model<A>>>::from_statement(stmt, values)
            .one(db)
            .await?;

        Ok(found)
    }

    async fn exec_update_with_returning<E, C>(mut self, db: &C) -> Result<Vec<E::Model>, DbErr>
    where
        E: EntityTrait,
        C: ConnectionTrait,
    {
        if self.is_noop() {
            return Ok(vec![]);
        }

        let returning = Query::returning()
            .exprs(E::Column::iter().map(|c| c.select_as(c.into_returning_expr())));

        self.query.returning(returning);

        let (stmt, values) = self.query.build(QueryBuilder);

        let models: Vec<E::Model> =
            SelectorRaw::<SelectModel<E::Model>>::from_statement(stmt, values)
                .all(db)
                .await?;

        Ok(models)
    }

    fn is_noop(&self) -> bool {
        self.query.get_values().is_empty()
    }
}

// [spec:pgorm:sem:exec.crud.update]
async fn find_updated_model_by_id<A, C>(
    model: A,
    db: &C,
) -> Result<<A::Entity as EntityTrait>::Model, DbErr>
where
    A: ActiveModelTrait,
    C: ConnectionTrait,
{
    type Entity<A> = <A as ActiveModelTrait>::Entity;
    type ValueType<A> = <<Entity<A> as EntityTrait>::PrimaryKey as PrimaryKeyTrait>::ValueType;

    let primary_key_value = match model.get_primary_key_value() {
        Some(val) => ValueType::<A>::from_value_tuple(val),
        None => return Err(DbErr::UpdateGetPrimaryKey),
    };
    let found = Entity::<A>::find_by_id(primary_key_value).one(db).await?;

    Ok(found)
}
