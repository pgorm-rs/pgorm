use crate::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DbErr, EntityTrait, IntoActiveModel, Iterable,
    PrimaryKeyToColumn, QueryFilter, QueryTrait,
};
use core::marker::PhantomData;
use pgorm_query::DeleteStatement;

/// Defines the structure for a delete operation
#[derive(Clone, Debug)]
pub struct Delete;

/// Perform a delete operation on a model
#[derive(Clone, Debug)]
pub struct DeleteOne<A>
where
    A: ActiveModelTrait,
{
    pub(crate) query: DeleteStatement,
    pub(crate) model: A,
}

/// Perform a delete operation on multiple models
#[derive(Clone, Debug)]
pub struct DeleteMany<E>
where
    E: EntityTrait,
{
    pub(crate) query: DeleteStatement,
    pub(crate) entity: PhantomData<E>,
}

// [spec:pgorm:sem:query.build.delete+1]
impl Delete {
    /// Delete one Model or ActiveModel
    ///
    /// Fails with [`DbErr::PrimaryKeyNotSet`] when a primary-key column of
    /// `model` is [`ActiveValue::NotSet`], since there would be nothing to
    /// narrow the statement to a single row.
    ///
    /// Model
    /// ```
    /// use pgorm::{entity::*, pgorm_query::QueryBuilder, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     Delete::one(cake::Model {
    ///         id: 1,
    ///         name: "Apple Pie".to_owned(),
    ///     })
    ///     .expect("the primary key is set")
    ///     .as_query()
    ///     .to_string(QueryBuilder),
    ///     r#"DELETE FROM "cake" WHERE "cake"."id" = 1"#,
    /// );
    /// ```
    /// ActiveModel
    /// ```
    /// use pgorm::{entity::*, pgorm_query::QueryBuilder, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     Delete::one(cake::ActiveModel {
    ///         id: ActiveValue::set(1),
    ///         name: ActiveValue::set("Apple Pie".to_owned()),
    ///     })
    ///     .expect("the primary key is set")
    ///     .as_query()
    ///     .to_string(QueryBuilder),
    ///     r#"DELETE FROM "cake" WHERE "cake"."id" = 1"#,
    /// );
    /// ```
    ///
    /// ```
    /// use pgorm::{entity::*, error::DbErr, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     Delete::one(cake::ActiveModel {
    ///         id: ActiveValue::not_set(),
    ///         name: ActiveValue::set("Apple Pie".to_owned()),
    ///     })
    ///     .unwrap_err(),
    ///     DbErr::PrimaryKeyNotSet,
    /// );
    /// ```
    pub fn one<E, A, M>(model: M) -> Result<DeleteOne<A>, DbErr>
    where
        E: EntityTrait,
        A: ActiveModelTrait<Entity = E>,
        M: IntoActiveModel<A>,
    {
        let myself = DeleteOne {
            query: DeleteStatement::new()
                .from_table(A::Entity::default().table_ref())
                .to_owned(),
            model: model.into_active_model(),
        };
        myself.prepare_filters()
    }

    /// Delete many ActiveModel
    ///
    /// ```
    /// use pgorm::{entity::*, pgorm_query::QueryBuilder, query::*, tests_cfg::fruit};
    ///
    /// assert_eq!(
    ///     Delete::many(fruit::Entity)
    ///         .filter(fruit::Column::Name.contains("Apple"))
    ///         .as_query()
    ///         .to_string(QueryBuilder),
    ///     r#"DELETE FROM "fruit" WHERE "fruit"."name" LIKE '%Apple%'"#,
    /// );
    /// ```
    pub fn many<E>(entity: E) -> DeleteMany<E>
    where
        E: EntityTrait,
    {
        DeleteMany {
            query: DeleteStatement::new()
                .from_table(entity.table_ref())
                .to_owned(),
            entity: PhantomData,
        }
    }
}

// [spec:pgorm:sem:query.build.delete+1]
impl<A> DeleteOne<A>
where
    A: ActiveModelTrait,
{
    fn prepare_filters(mut self) -> Result<Self, DbErr> {
        for key in <A::Entity as EntityTrait>::PrimaryKey::iter() {
            let col = key.into_column();
            match self.model.get(col) {
                ActiveValue::Set(value) | ActiveValue::Unchanged(value) => {
                    self = self.filter(col.eq(value));
                }
                ActiveValue::NotSet => return Err(DbErr::PrimaryKeyNotSet),
            }
        }
        Ok(self)
    }
}

impl<A> QueryFilter for DeleteOne<A>
where
    A: ActiveModelTrait,
{
    type QueryStatement = DeleteStatement;

    fn query(&mut self) -> &mut DeleteStatement {
        &mut self.query
    }
}

impl<E> QueryFilter for DeleteMany<E>
where
    E: EntityTrait,
{
    type QueryStatement = DeleteStatement;

    fn query(&mut self) -> &mut DeleteStatement {
        &mut self.query
    }
}

impl<A> QueryTrait for DeleteOne<A>
where
    A: ActiveModelTrait,
{
    type QueryStatement = DeleteStatement;

    fn query(&mut self) -> &mut DeleteStatement {
        &mut self.query
    }

    fn as_query(&self) -> &DeleteStatement {
        &self.query
    }

    fn into_query(self) -> DeleteStatement {
        self.query
    }
}

impl<E> QueryTrait for DeleteMany<E>
where
    E: EntityTrait,
{
    type QueryStatement = DeleteStatement;

    fn query(&mut self) -> &mut DeleteStatement {
        &mut self.query
    }

    fn as_query(&self) -> &DeleteStatement {
        &self.query
    }

    fn into_query(self) -> DeleteStatement {
        self.query
    }
}

#[cfg(test)]
mod tests {
    use crate::tests_cfg::{cake, fruit};
    use crate::{entity::*, query::*};
    use pgorm_query::QueryBuilder;

    #[test]
    fn delete_1() {
        assert_eq!(
            Delete::one(cake::Model {
                id: 1,
                name: "Apple Pie".to_owned(),
            })
            .expect("the primary key is set")
            .as_query()
            .to_string(QueryBuilder),
            r#"DELETE FROM "cake" WHERE "cake"."id" = 1"#,
        );
        assert_eq!(
            Delete::one(cake::ActiveModel {
                id: ActiveValue::set(1),
                name: ActiveValue::set("Apple Pie".to_owned()),
            })
            .expect("the primary key is set")
            .as_query()
            .to_string(QueryBuilder),
            r#"DELETE FROM "cake" WHERE "cake"."id" = 1"#,
        );
    }

    #[test]
    fn delete_2() {
        assert_eq!(
            Delete::many(fruit::Entity)
                .filter(fruit::Column::Name.contains("Cheese"))
                .as_query()
                .to_string(QueryBuilder),
            r#"DELETE FROM "fruit" WHERE "fruit"."name" LIKE '%Cheese%'"#,
        );
    }
}
