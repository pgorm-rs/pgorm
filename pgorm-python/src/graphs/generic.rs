use crate::execution::Database;
use futures_util::future::BoxFuture;
use pgorm::pgorm_query::{ValueTuple, Values};
use pgorm::{
    EntityTrait, Error, GraphItem, GraphRow, IdenStr, Iterable, QueryFilter, QueryOrder,
    QueryTrait, SelectGraph, SelectorTrait,
};
use std::{marker::PhantomData, sync::Arc};

use super::{backend::*, slots::GraphSlots};

pub(crate) struct GraphFactory<E: EntityTrait, S, Build> {
    pub(crate) info: Arc<Info>,
    pub(crate) build: Build,
    pub(crate) marker: PhantomData<(E, S)>,
}

impl<E: EntityTrait, S, Build> std::fmt::Debug for GraphFactory<E, S, Build> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphFactory")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

impl<E, S, Build> Factory for GraphFactory<E, S, Build>
where
    E: EntityTrait + Send + Sync + 'static,
    S: GraphSlots<E>,
    GraphRow<E, S>: SelectorTrait,
    GraphItem<E, S>: Send + 'static,
    Build: Fn(&[String]) -> SelectGraph<E, S> + Send + Sync + 'static,
{
    fn info(&self) -> &Arc<Info> {
        &self.info
    }

    fn find(&self, aliases: Vec<String>) -> Query {
        Arc::new(GraphQuery::<E, S> {
            query: (self.build)(&aliases),
            info: self.info.clone(),
            aliases,
        })
    }
}

#[derive(Debug)]
struct GraphQuery<E: EntityTrait, S> {
    query: SelectGraph<E, S>,
    info: Arc<Info>,
    aliases: Vec<String>,
}

// [spec:pgorm:req:python.graph]
impl<E, S> QueryBackend for GraphQuery<E, S>
where
    E: EntityTrait + Send + Sync + 'static,
    S: GraphSlots<E>,
    GraphRow<E, S>: SelectorTrait,
    GraphItem<E, S>: Send + 'static,
{
    fn info(&self) -> &Arc<Info> {
        &self.info
    }
    fn aliases(&self) -> &[String] {
        &self.aliases
    }

    fn change(&self, change: Change) -> Query {
        let query = self.query.clone();
        let query = match change {
            Change::Filter(condition) => query.filter(condition),
            Change::Order(expr, order, None) => query.order_by(expr, order),
            Change::Order(expr, order, Some(nulls)) => {
                query.order_by_with_nulls(expr, order, nulls)
            }
        };
        Arc::new(Self {
            query,
            info: self.info.clone(),
            aliases: self.aliases.clone(),
        })
    }

    fn compile(&self, optional: bool) -> (String, Values) {
        let mut query = self.query.clone();
        if optional {
            QueryTrait::query(&mut query).limit(1);
        }
        query.build()
    }

    fn run<'a>(
        &'a self,
        db: Database<'a>,
        optional: bool,
    ) -> BoxFuture<'a, Result<Vec<Row>, Error>> {
        Box::pin(self.read(db, optional))
    }

    fn cursor<'a>(
        &'a self,
        db: Database<'a>,
        plan: CursorPlan,
    ) -> BoxFuture<'a, Result<Vec<Row>, Error>> {
        Box::pin(self.read_cursor(db, plan))
    }
}

impl<E, S> GraphQuery<E, S>
where
    E: EntityTrait + Send + Sync + 'static,
    S: GraphSlots<E>,
    GraphRow<E, S>: SelectorTrait,
    GraphItem<E, S>: Send + 'static,
{
    async fn read(&self, db: Database<'_>, optional: bool) -> Result<Vec<Row>, Error> {
        let rows = if optional {
            self.query.clone().one_opt(&db).await?.into_iter().collect()
        } else {
            self.query.clone().all(&db).await?
        };
        Ok(rows
            .into_iter()
            .map(|row| S::models(row, &self.info.bindings))
            .collect())
    }

    async fn read_cursor(&self, db: Database<'_>, plan: CursorPlan) -> Result<Vec<Row>, Error> {
        let column = E::Column::iter()
            .find(|column| column.as_str() == plan.column)
            .ok_or_else(|| Error::Type("unknown root cursor column".into()))?;
        let mut cursor = self.query.clone().cursor_by(column);
        for (before, bound) in [(true, plan.before), (false, plan.after)] {
            let Some(bound) = bound else {
                continue;
            };
            if bound.full {
                let values: ValueTuple = bound.values.into_iter().collect();
                if before {
                    cursor.before_with(values);
                } else {
                    cursor.after_with(values);
                }
            } else {
                let mut values = bound.values.into_iter();
                let value = values
                    .next()
                    .ok_or_else(|| Error::Type("missing primary cursor boundary".into()))?;
                if values.next().is_some() {
                    return Err(Error::Type(
                        "primary cursor boundary has excess values".into(),
                    ));
                }
                if before {
                    cursor.before(value);
                } else {
                    cursor.after(value);
                }
            }
        }
        if plan.descending {
            cursor.desc();
        } else {
            cursor.asc();
        }
        if let Some((last, count)) = plan.window {
            if last {
                cursor.last(count);
            } else {
                cursor.first(count);
            }
        }
        Ok(cursor
            .all(&db)
            .await?
            .into_iter()
            .map(|row| S::models(row, &self.info.bindings))
            .collect())
    }
}
