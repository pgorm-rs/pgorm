use futures_util::future::BoxFuture;
use pgorm::{
    DatabaseConnection, Error,
    pgorm_query::{Condition, NullOrdering, Order, SimpleExpr, Value, Values},
};
use std::{fmt::Debug, sync::Arc};

use super::slots::GraphBindings;
use crate::entities::PyEntityModel;

pub(crate) type Row = Vec<Option<PyEntityModel>>;
pub(crate) type Query = Arc<dyn QueryBackend>;

#[derive(Clone, Debug)]
pub(crate) struct Info {
    pub(crate) name: String,
    pub(crate) rust_shape: &'static str,
    pub(crate) bindings: GraphBindings,
}

impl Info {
    pub(crate) fn describe(&self) -> serde_json::Value {
        serde_json::json!({"name": self.name, "rust_shape": self.rust_shape,
            "sources": self.bindings.sources.iter().enumerate().map(|(index, source)| {
                serde_json::json!({"index":index, "entity":source.entity.name,
                    "slot": if index == 0 { "root" } else if source.optional { "Opt" } else { "Req" }})
            }).collect::<Vec<_>>(),
            "terminals": ["all", "one_opt", "cursor.all"],
            "aliases": "joined sources at find time; root name is fixed by EntityTrait",
            "cursor": {"source": 0, "order_columns": 1, "bounds": ["before", "after", "before_with", "after_with"], "windows": ["first", "last"], "inspect": false}})
    }
}

pub(crate) trait Factory: Debug + Send + Sync {
    fn info(&self) -> &Arc<Info>;
    fn find(&self, aliases: Vec<String>) -> Query;
}

#[derive(Clone, Debug)]
pub(crate) enum Change {
    Filter(Condition),
    Order(SimpleExpr, Order, Option<NullOrdering>),
}

#[derive(Clone, Debug)]
pub(crate) struct Boundary {
    pub(crate) values: Vec<Value>,
    pub(crate) full: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CursorPlan {
    pub(crate) column: String,
    pub(crate) before: Option<Boundary>,
    pub(crate) after: Option<Boundary>,
    pub(crate) descending: bool,
    pub(crate) window: Option<(bool, u64)>,
}

pub(crate) trait QueryBackend: Debug + Send + Sync {
    fn info(&self) -> &Arc<Info>;
    fn aliases(&self) -> &[String];
    fn change(&self, change: Change) -> Query;
    fn compile(&self, optional: bool) -> (String, Values);
    fn run<'a>(
        &'a self,
        db: &'a DatabaseConnection,
        optional: bool,
    ) -> BoxFuture<'a, Result<Vec<Row>, Error>>;
    fn cursor<'a>(
        &'a self,
        db: &'a DatabaseConnection,
        plan: CursorPlan,
    ) -> BoxFuture<'a, Result<Vec<Row>, Error>>;
}
