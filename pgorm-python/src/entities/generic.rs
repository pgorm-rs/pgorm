//! Each adapter below is monomorphized at downstream module build time.

use std::{marker::PhantomData, sync::Arc};

use futures_util::future::BoxFuture;
use pgorm::pgorm_query::{Expr, SimpleExpr, Value, Values};
use pgorm::{
    ActiveModelBehavior, ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection,
    EntityTrait, Error, IdenStr, IntoActiveModel, Iterable, ModelTrait, QueryFilter, QueryOrder,
    QuerySelect, QueryTrait,
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

// [spec:pgorm:req:python.entities]
impl<E> EntityBackend for EntityAdapter<E>
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    fn info(&self) -> &Arc<EntityInfo> {
        &self.info
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
        db: &'a DatabaseConnection,
        terminal: Terminal,
    ) -> BoxFuture<'a, Result<Vec<Model>, Error>> {
        Box::pin(async move {
            let query = self.query.clone();
            let models = match terminal {
                Terminal::All => query.all(db).await?,
                Terminal::One => vec![query.one(db).await?],
                Terminal::Optional => query.one_opt(db).await?.into_iter().collect(),
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
        })
    }
}

#[derive(Debug)]
struct ModelAdapter<E: EntityTrait> {
    value: E::Model,
    info: Arc<EntityInfo>,
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
        Ok(self.value.get(column::<E>(name)?))
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

    fn run<'a>(
        &'a self,
        db: &'a DatabaseConnection,
        write: Write,
    ) -> BoxFuture<'a, Result<Written, Error>> {
        Box::pin(async move {
            let model = match write {
                Write::Insert => self.value.clone().insert(db).await?,
                Write::Update => self.value.clone().update(db).await?,
                Write::Delete => return self.value.clone().delete(db).await.map(Written::Count),
            };
            Ok(Written::Model(Arc::new(ModelAdapter::<E> {
                value: model,
                info: self.info.clone(),
            })))
        })
    }
}
