//! Each adapter below is monomorphized at downstream module build time.

use crate::execution::Database;
use std::{marker::PhantomData, sync::Arc};

use futures_util::future::BoxFuture;
use pgorm::pgorm_query::{Expr, SimpleExpr, Value, Values};
use pgorm::{
    ActiveModelBehavior, ActiveModelTrait, ActiveValue, Change as RustChange, ColumnTrait,
    EntityTrait, Error, Insert, IntoActiveModel, Iterable, ModelTrait, QueryFilter, QueryOrder,
    QuerySelect, QueryTrait, StaticName, Update, Upserted,
};

use super::{backend::*, metadata::EntityInfo};

fn column<E: EntityTrait>(name: &str) -> Result<E::Column, Error> {
    E::Column::iter()
        .find(|c| c.as_str() == name)
        .ok_or_else(|| Error::Type("unknown compiled entity column".to_owned()))
}

#[derive(Debug)]
pub(crate) struct EntityAdapter<E> {
    pub(crate) info: Arc<EntityInfo>,
    pub(crate) entity: PhantomData<E>,
}

// [spec:pgorm:req:python.entities+2]
impl<E> EntityBackend for EntityAdapter<E>
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    fn info(&self) -> &Arc<EntityInfo> {
        &self.info
    }

    // [spec:pgorm:req:python.schema]
    fn schema(&self) -> crate::schema::PyEntitySchema {
        use crate::schema::{PyCreateIndex, PyCreateTable, PyDDL, PyEntitySchema, Statement};
        let schema = pgorm::Schema::new();
        PyEntitySchema {
            table: PyCreateTable {
                inner: schema.create_table_from_entity(E::default()),
            },
            enums: schema
                .create_enum_from_entity(E::default())
                .into_iter()
                .map(|inner| PyDDL {
                    inner: Statement::CreateEnum(inner),
                })
                .collect(),
            indexes: schema
                .create_index_from_entity(E::default())
                .into_iter()
                .map(|inner| PyCreateIndex { inner })
                .collect(),
            comments: schema
                .create_comments_from_entity(E::default())
                .into_iter()
                .map(|inner| PyDDL {
                    inner: Statement::Comment(inner),
                })
                .collect(),
        }
    }

    fn select(&self) -> Select {
        Arc::new(SelectAdapter::<E> {
            query: E::find(),
            info: self.info.clone(),
        })
    }

    fn active(&self) -> Active {
        Arc::new(ActiveAdapter::<E> {
            value: E::ActiveModel::new(),
            info: self.info.clone(),
        })
    }

    fn expression(&self, name: &str) -> Result<SimpleExpr, Error> {
        Ok(Expr::col(column::<E>(name)?.as_column_ref()).into())
    }

    fn compare(
        &self,
        name: &str,
        comparison: Comparison,
        value: Value,
    ) -> Result<SimpleExpr, Error> {
        let column = column::<E>(name)?;
        Ok(match comparison {
            Comparison::Eq => column.eq(value),
            Comparison::Ne => column.ne(value),
            Comparison::Gt => column.gt(value),
            Comparison::Ge => column.gte(value),
            Comparison::Lt => column.lt(value),
            Comparison::Le => column.lte(value),
        })
    }

    fn versions<'a>(
        &'a self,
        db: Database<'a>,
        write: VersionWrite,
    ) -> BoxFuture<'a, Result<Vec<Versions>, Error>> {
        Box::pin(self.write_versions(db, write))
    }
}

/// The compiled ActiveModel a type-erased handle holds, refused if it was
/// minted by another registration.
fn concrete<E>(active: &Active) -> Result<E::ActiveModel, Error>
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    active
        .as_any()
        .downcast_ref::<ActiveAdapter<E>>()
        .map(|adapter| adapter.value.clone())
        .ok_or_else(|| Error::Type("ActiveModel belongs to another entity registration".to_owned()))
}

impl<E> EntityAdapter<E>
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    fn model(&self, value: E::Model) -> Model {
        Arc::new(ModelAdapter::<E> {
            value,
            info: self.info.clone(),
        })
    }

    fn change(&self, change: RustChange<E::Model>) -> Versions {
        Versions {
            old: Some(self.model(change.old)),
            new: self.model(change.new),
        }
    }

    fn upserted(&self, upserted: Upserted<E::Model>) -> Versions {
        match upserted {
            Upserted::Inserted(model) => Versions {
                old: None,
                new: self.model(model),
            },
            Upserted::Updated(change) => self.change(change),
        }
    }

    async fn write_versions(
        &self,
        db: Database<'_>,
        write: VersionWrite,
    ) -> Result<Vec<Versions>, Error> {
        match write {
            VersionWrite::Change(active) => {
                let change = Update::one(concrete::<E>(&active)?)?
                    .exec_returning_change(&db)
                    .await?;
                Ok(vec![self.change(change)])
            }
            VersionWrite::Changes(assignments, filter) => {
                let mut update = Update::many(E::default());
                for (name, assignment) in assignments {
                    let column = column::<E>(&name)?;
                    let expression = match assignment {
                        Assignment::Value(value) => column.save_as(Expr::val(value)),
                        Assignment::Expr(expression) => expression,
                    };
                    update = update.col_expr(column, expression);
                }
                let changes = update.filter(filter).exec_returning_changes(&db).await?;
                Ok(changes
                    .into_iter()
                    .map(|change| self.change(change))
                    .collect())
            }
            VersionWrite::Upserts {
                actives,
                conflict,
                one,
            } => {
                let models = actives
                    .iter()
                    .map(concrete::<E>)
                    .collect::<Result<Vec<_>, _>>()?;
                let upserted = if one {
                    let model = models.into_iter().next().ok_or_else(|| {
                        Error::Type("an upsert of one row needs one ActiveModel".to_owned())
                    })?;
                    let mut insert = Insert::one(model);
                    if let Some(conflict) = conflict {
                        insert = insert.on_conflict(*conflict);
                    }
                    insert
                        .exec_returning_upsert(&db)
                        .await?
                        .into_iter()
                        .collect()
                } else {
                    let mut insert = Insert::many(models);
                    if let Some(conflict) = conflict {
                        insert = insert.on_conflict(*conflict);
                    }
                    insert.exec_returning_upserts(&db).await?
                };
                Ok(upserted.into_iter().map(|row| self.upserted(row)).collect())
            }
        }
    }
}

#[derive(Debug)]
struct SelectAdapter<E: EntityTrait> {
    query: pgorm::Select<E>,
    info: Arc<EntityInfo>,
}

impl<E> SelectBackend for SelectAdapter<E>
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    fn info(&self) -> &Arc<EntityInfo> {
        &self.info
    }

    fn change(&self, change: Change) -> Select {
        let query = self.query.clone();
        let query = match change {
            Change::Filter(condition) => query.filter(condition),
            Change::Order(expr, order, Some(nulls)) => {
                query.order_by_with_nulls(expr, order, nulls)
            }
            Change::Order(expr, order, None) => query.order_by(expr, order),
            Change::Limit(limit) => query.limit(limit),
            Change::Offset(offset) => query.offset(offset),
        };
        Arc::new(Self {
            query,
            info: self.info.clone(),
        })
    }

    fn compile(&self, terminal: Terminal) -> (String, Values) {
        let mut query = self.query.clone();
        if !matches!(terminal, Terminal::All) {
            query = query.limit(1);
        }
        query.build()
    }

    fn run<'a>(
        &'a self,
        db: Database<'a>,
        terminal: Terminal,
    ) -> BoxFuture<'a, Result<Vec<Model>, Error>> {
        Box::pin(self.read(db, terminal))
    }
}

#[derive(Debug)]
pub(crate) struct ModelAdapter<E: EntityTrait> {
    pub(crate) value: E::Model,
    pub(crate) info: Arc<EntityInfo>,
}

impl<E> ModelBackend for ModelAdapter<E>
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    fn info(&self) -> &Arc<EntityInfo> {
        &self.info
    }

    fn get(&self, name: &str) -> Result<Value, Error> {
        Ok(self.value.get(column::<E>(name)?))
    }

    fn set(&self, name: &str, value: Value) -> Result<Model, Error> {
        let mut model = self.value.clone();
        model.set(column::<E>(name)?, value)?;
        Ok(Arc::new(Self {
            value: model,
            info: self.info.clone(),
        }))
    }

    fn active(&self) -> Active {
        Arc::new(ActiveAdapter::<E> {
            value: self.value.clone().into_active_model(),
            info: self.info.clone(),
        })
    }
}

#[derive(Debug)]
struct ActiveAdapter<E: EntityTrait> {
    value: E::ActiveModel,
    info: Arc<EntityInfo>,
}

impl<E> ActiveBackend for ActiveAdapter<E>
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    fn info(&self) -> &Arc<EntityInfo> {
        &self.info
    }

    fn get(&self, name: &str) -> Result<ActiveValue<Value>, Error> {
        self.value.get(column::<E>(name)?)
    }

    fn set(&self, name: &str, value: Value) -> Result<Active, Error> {
        let mut active = self.value.clone();
        active.set(column::<E>(name)?, value)?;
        Ok(Arc::new(Self {
            value: active,
            info: self.info.clone(),
        }))
    }

    fn not_set(&self, name: &str) -> Result<Active, Error> {
        let mut active = self.value.clone();
        active.not_set(column::<E>(name)?);
        Ok(Arc::new(Self {
            value: active,
            info: self.info.clone(),
        }))
    }

    fn reset(&self, name: &str) -> Result<Active, Error> {
        let mut active = self.value.clone();
        active.reset(column::<E>(name)?);
        Ok(Arc::new(Self {
            value: active,
            info: self.info.clone(),
        }))
    }

    fn run<'a>(&'a self, db: Database<'a>, write: Write) -> BoxFuture<'a, Result<Written, Error>> {
        Box::pin(self.write(db, write))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl<E> SelectAdapter<E>
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    async fn read(&self, db: Database<'_>, terminal: Terminal) -> Result<Vec<Model>, Error> {
        let query = self.query.clone();
        let models = match terminal {
            Terminal::All => query.all(&db).await?,
            Terminal::One => vec![query.one(&db).await?],
            Terminal::Optional => query.one_opt(&db).await?.into_iter().collect(),
        };
        Ok(models
            .into_iter()
            .map(|value| {
                Arc::new(ModelAdapter::<E> {
                    value,
                    info: self.info.clone(),
                }) as Model
            })
            .collect())
    }
}

impl<E> ActiveAdapter<E>
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    async fn write(&self, db: Database<'_>, write: Write) -> Result<Written, Error> {
        let model = match write {
            Write::Insert => self.value.clone().insert(&db).await?,
            Write::Update => self.value.clone().update(&db).await?,
            Write::Delete => return self.value.clone().delete(&db).await.map(Written::Count),
        };
        Ok(Written::Model(Arc::new(ModelAdapter::<E> {
            value: model,
            info: self.info.clone(),
        })))
    }
}
