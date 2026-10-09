//! Registered `SelectGraph<E, S>` shapes: the root entity and its tuple of
//! `Req<F>` and `Opt<F>` slots fixed in Rust, with a factory the application
//! supplies to build the graph under the aliases JavaScript chooses. Rust
//! writes the projection and decodes each row through `GraphRow`; JavaScript
//! varies filters, ordering, aliases and cursor bounds within that shape.

use std::{fmt, marker::PhantomData, sync::Arc};

use futures_util::future::BoxFuture;
use pgorm::pgorm_query::{Condition, NullOrdering, Order, SimpleExpr, Value, ValueTuple, Values};
use pgorm::{
    EntityTrait, Error, GraphItem, GraphRow, IntoActiveModel, Iterable, Opt, QueryFilter,
    QueryOrder, QueryTrait, Req, SelectGraph, SelectorTrait, Slot, Slots, StaticName,
};

use super::{
    Registry,
    adapter::{Model, model_of},
    info::EntityInfo,
    registry::RegistrationError,
};
use crate::connect::job::Database;

/// One decoded source of a registered graph: its entity, and whether its
/// row may be absent.
#[derive(Clone, Debug)]
pub struct Source {
    pub(crate) entity: Arc<EntityInfo>,
    pub(crate) optional: bool,
}

/// One decoded model of a graph row, behind its type-erased handle.
#[derive(Clone, Debug)]
pub struct GraphModel(pub(crate) Model);

/// The registered entities a graph's tuple decodes, root first. Produced by
/// [`GraphSlots`]; applications do not construct it.
#[derive(Clone, Debug)]
pub struct GraphBindings {
    pub(crate) sources: Vec<Source>,
}

#[doc(hidden)]
pub trait RegisteredSlot: Slot {
    fn source(registry: &Registry) -> Result<Source, RegistrationError>;
    fn model(value: Self::Out, source: &Source) -> Option<GraphModel>;
}

impl<F> RegisteredSlot for Req<F>
where
    F: EntityTrait + Send + Sync + 'static,
    F::Model: IntoActiveModel<F::ActiveModel> + Sync + 'static,
    F::ActiveModel: Send + Sync + 'static,
{
    fn source(registry: &Registry) -> Result<Source, RegistrationError> {
        Ok(Source {
            entity: registry.registered::<F>()?,
            optional: false,
        })
    }

    fn model(value: Self::Out, source: &Source) -> Option<GraphModel> {
        Some(GraphModel(model_of::<F>(value, &source.entity)))
    }
}

impl<F> RegisteredSlot for Opt<F>
where
    F: EntityTrait + Send + Sync + 'static,
    F::Model: IntoActiveModel<F::ActiveModel> + Sync + 'static,
    F::ActiveModel: Send + Sync + 'static,
{
    fn source(registry: &Registry) -> Result<Source, RegistrationError> {
        Ok(Source {
            entity: registry.registered::<F>()?,
            optional: true,
        })
    }

    fn model(value: Self::Out, source: &Source) -> Option<GraphModel> {
        value.map(|value| GraphModel(model_of::<F>(value, &source.entity)))
    }
}

/// The slot tuples a graph can register: Rust's own arities, from no slot to
/// six. pgorm's `Slots` is sealed, so no other shape implements this.
// [spec:pgorm:req:napi.entity-graphs]
pub trait GraphSlots<E: EntityTrait>: Slots + fmt::Debug + Send + Sync + Sized + 'static
where
    GraphRow<E, Self>: SelectorTrait,
{
    #[doc(hidden)]
    fn bindings(registry: &Registry) -> Result<GraphBindings, RegistrationError>;
    #[doc(hidden)]
    fn models(row: GraphItem<E, Self>, bindings: &GraphBindings) -> Vec<Option<GraphModel>>;
}

impl<E> GraphSlots<E> for ()
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    fn bindings(registry: &Registry) -> Result<GraphBindings, RegistrationError> {
        Ok(GraphBindings {
            sources: vec![Source {
                entity: registry.registered::<E>()?,
                optional: false,
            }],
        })
    }

    fn models(value: E::Model, bindings: &GraphBindings) -> Vec<Option<GraphModel>> {
        vec![
            bindings
                .sources
                .first()
                .map(|source| GraphModel(model_of::<E>(value, &source.entity))),
        ]
    }
}

macro_rules! slots {
    ($($slot:ident @ $at:tt),+) => {
        impl<E, $($slot),+> GraphSlots<E> for ($($slot,)+)
        where
            E: EntityTrait + Send + Sync + 'static,
            E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
            E::ActiveModel: Send + Sync + 'static,
            $($slot: RegisteredSlot + fmt::Debug + Send + Sync + 'static,)+
        {
            fn bindings(registry: &Registry) -> Result<GraphBindings, RegistrationError> {
                Ok(GraphBindings {
                    sources: vec![
                        Source { entity: registry.registered::<E>()?, optional: false },
                        $($slot::source(registry)?,)+
                    ],
                })
            }

            fn models(row: GraphItem<E, Self>, bindings: &GraphBindings) -> Vec<Option<GraphModel>> {
                let sources = &bindings.sources;
                vec![
                    sources.first().map(|source| GraphModel(model_of::<E>(row.0, &source.entity))),
                    $(sources.get($at).and_then(|source| $slot::model(row.$at, source)),)+
                ]
            }
        }
    };
}

slots!(S1 @ 1);
slots!(S1 @ 1, S2 @ 2);
slots!(S1 @ 1, S2 @ 2, S3 @ 3);
slots!(S1 @ 1, S2 @ 2, S3 @ 3, S4 @ 4);
slots!(S1 @ 1, S2 @ 2, S3 @ 3, S4 @ 4, S5 @ 5);
slots!(S1 @ 1, S2 @ 2, S3 @ 3, S4 @ 4, S5 @ 5, S6 @ 6);

/// A registered graph: its name, the Rust shape and its sources.
#[derive(Debug)]
pub(crate) struct GraphInfo {
    pub(crate) name: String,
    pub(crate) shape: &'static str,
    pub(crate) bindings: GraphBindings,
}

impl GraphInfo {
    pub(crate) fn describe(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "rustShape": self.shape,
            "sources": self.bindings.sources.iter().enumerate().map(|(index, source)| {
                serde_json::json!({
                    "index": index,
                    "entity": source.entity.name,
                    "slot": if index == 0 { "root" } else if source.optional { "Opt" } else { "Req" },
                })
            }).collect::<Vec<_>>(),
        })
    }
}

pub(crate) type Query = Arc<dyn QueryBackend>;
pub(crate) type Row = Vec<Option<GraphModel>>;

pub(crate) trait Factory: fmt::Debug + Send + Sync {
    fn info(&self) -> &Arc<GraphInfo>;
    fn find(&self, aliases: Vec<String>) -> Query;
}

/// One step a graph query takes, each the `SelectGraph` method of its
/// meaning.
#[derive(Clone, Debug)]
pub(crate) enum Change {
    Filter(Condition),
    Order(SimpleExpr, Order, Option<NullOrdering>),
}

/// A cursor boundary: the root's order-column value, or with `full` the
/// whole key — the order column, the root's remaining key and each slot's.
#[derive(Clone, Debug)]
pub(crate) struct Boundary {
    pub(crate) values: Vec<Value>,
    pub(crate) full: bool,
}

/// What `SelectGraph::cursor_by` is told before `Cursor::all`.
#[derive(Clone, Debug, Default)]
pub(crate) struct CursorPlan {
    pub(crate) column: String,
    pub(crate) before: Option<Boundary>,
    pub(crate) after: Option<Boundary>,
    pub(crate) descending: bool,
    pub(crate) window: Option<(bool, u64)>,
}

pub(crate) trait QueryBackend: fmt::Debug + Send + Sync {
    fn info(&self) -> &Arc<GraphInfo>;
    fn aliases(&self) -> &[String];
    fn change(&self, change: Change) -> Query;
    fn compile(&self, optional: bool) -> (String, Values);
    fn run<'a>(
        &'a self,
        db: Database<'a>,
        optional: bool,
    ) -> BoxFuture<'a, Result<Vec<Row>, Error>>;
    fn cursor<'a>(
        &'a self,
        db: Database<'a>,
        plan: CursorPlan,
    ) -> BoxFuture<'a, Result<Vec<Row>, Error>>;
}

pub(crate) struct GraphFactory<E: EntityTrait, S, Build> {
    pub(crate) info: Arc<GraphInfo>,
    pub(crate) build: Build,
    pub(crate) marker: PhantomData<(E, S)>,
}

impl<E: EntityTrait, S, Build> fmt::Debug for GraphFactory<E, S, Build> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    fn info(&self) -> &Arc<GraphInfo> {
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

struct GraphQuery<E: EntityTrait, S> {
    query: SelectGraph<E, S>,
    info: Arc<GraphInfo>,
    aliases: Vec<String>,
}

impl<E: EntityTrait, S> fmt::Debug for GraphQuery<E, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphQuery")
            .field("info", &self.info)
            .field("aliases", &self.aliases)
            .finish_non_exhaustive()
    }
}

// [spec:pgorm:req:napi.entity-graphs]
impl<E, S> QueryBackend for GraphQuery<E, S>
where
    E: EntityTrait + Send + Sync + 'static,
    S: GraphSlots<E>,
    GraphRow<E, S>: SelectorTrait,
    GraphItem<E, S>: Send + 'static,
{
    fn info(&self) -> &Arc<GraphInfo> {
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
        Box::pin(async move {
            let rows = if optional {
                self.query.clone().one_opt(&db).await?.into_iter().collect()
            } else {
                self.query.clone().all(&db).await?
            };
            Ok(rows
                .into_iter()
                .map(|row| S::models(row, &self.info.bindings))
                .collect())
        })
    }

    fn cursor<'a>(
        &'a self,
        db: Database<'a>,
        plan: CursorPlan,
    ) -> BoxFuture<'a, Result<Vec<Row>, Error>> {
        Box::pin(async move {
            let column = <E::Column as Iterable>::iter()
                .find(|column| column.as_str() == plan.column)
                .ok_or_else(|| Error::Custom("unknown root cursor column".to_owned()))?;
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
                    let value = bound.values.into_iter().next().ok_or_else(|| {
                        Error::Custom("a cursor boundary needs its value".to_owned())
                    })?;
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
            match plan.window {
                Some((true, rows)) => {
                    cursor.last(rows);
                }
                Some((false, rows)) => {
                    cursor.first(rows);
                }
                None => {}
            }
            Ok(cursor
                .all(&db)
                .await?
                .into_iter()
                .map(|row| S::models(row, &self.info.bindings))
                .collect())
        })
    }
}
