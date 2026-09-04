use crate::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityName, EntityTrait, IntoActiveModel,
    Iterable, PrimaryKeyTrait, SelectModel, SelectorRaw, UpdateMany, UpdateOne, error::*,
};
use pgorm_query::{Query, TryFromValueTuple};
use tokio_postgres::types::ToSql;

use super::ValueHolder;

impl<A> UpdateOne<A>
where
    A: ActiveModelTrait,
{
    /// Execute the update and return the updated row as a model.
    ///
    /// `UpdateOne` has no bare `exec`: updating a single model by primary key
    /// always reads the row back. Use [`Update::many`](crate::Update::many)
    /// filtered to the key when a rows-affected count is all that is wanted.
    // [spec:pgorm:sem:exec.crud.update+5]
    // [spec:pgorm:sem:exec.crud.exec-vocabulary]
    pub async fn exec_returning_model<C>(
        mut self,
        db: &C,
    ) -> Result<<A::Entity as EntityTrait>::Model, Error>
    where
        <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
        C: ConnectionTrait,
    {
        type Entity<A> = <A as ActiveModelTrait>::Entity;
        type Model<A> = <Entity<A> as EntityTrait>::Model;
        type Column<A> = <Entity<A> as EntityTrait>::Column;

        if self.query.get_values().is_empty() {
            return find_updated_model_by_id(self.model, db).await;
        }

        let returning = Query::returning()
            .exprs(Column::<A>::iter().map(|c| c.select_as(c.into_returning_expr())));
        self.query.returning(returning);

        let (stmt, values) = self.query.build();

        let found: Model<A> = SelectorRaw::<SelectModel<Model<A>>>::from_statement(stmt, values)
            .one(db)
            .await?;

        Ok(found)
    }
}

impl<E> UpdateMany<E>
where
    E: EntityTrait,
{
    /// Execute the update and report how many rows it changed.
    ///
    /// No `RETURNING` clause is emitted. See [`Self::exec_returning_models`] for
    /// the updated rows.
    // [spec:pgorm:sem:exec.crud.update+5]
    // [spec:pgorm:sem:exec.crud.exec-vocabulary]
    pub async fn exec<C>(self, db: &C) -> Result<u64, Error>
    where
        C: ConnectionTrait,
    {
        if self.query.get_values().is_empty() {
            return Ok(0);
        }
        let (stmt, values) = self.query.build();
        let values = values.into_iter().map(ValueHolder).collect::<Vec<_>>();
        let values = values
            .iter()
            .map(|x| x as _)
            .collect::<Vec<&(dyn ToSql + Sync)>>();

        db.execute(&stmt, &values).await
    }

    /// Execute the update and return every updated row as a model.
    // [spec:pgorm:sem:exec.crud.update+5]
    // [spec:pgorm:sem:exec.crud.exec-vocabulary]
    pub async fn exec_returning_models<C>(mut self, db: &C) -> Result<Vec<E::Model>, Error>
    where
        C: ConnectionTrait,
    {
        if self.query.get_values().is_empty() {
            return Ok(vec![]);
        }

        let returning = Query::returning()
            .exprs(E::Column::iter().map(|c| c.select_as(c.into_returning_expr())));

        self.query.returning(returning);

        let (stmt, values) = self.query.build();

        let models: Vec<E::Model> =
            SelectorRaw::<SelectModel<E::Model>>::from_statement(stmt, values)
                .all(db)
                .await?;

        Ok(models)
    }
}

// [spec:pgorm:sem:exec.crud.update+5]
async fn find_updated_model_by_id<A, C>(
    model: A,
    db: &C,
) -> Result<<A::Entity as EntityTrait>::Model, Error>
where
    A: ActiveModelTrait,
    C: ConnectionTrait,
{
    type Entity<A> = <A as ActiveModelTrait>::Entity;
    type ValueType<A> = <<Entity<A> as EntityTrait>::PrimaryKey as PrimaryKeyTrait>::ValueType;

    let primary_key_value = match model.get_primary_key_value() {
        Some(val) => ValueType::<A>::try_from_value_tuple(val)
            .map_err(|err| primary_key_type_err(Entity::<A>::default().table_name(), err))?,
        None => return Err(Error::PrimaryKeyNotSet),
    };
    let found = Entity::<A>::find_by_id(primary_key_value).one(db).await?;

    Ok(found)
}
