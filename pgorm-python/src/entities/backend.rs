use std::{fmt::Debug, sync::Arc};

use futures_util::future::BoxFuture;
use pgorm::pgorm_query::{Condition, NullOrdering, Order, SimpleExpr, Value, Values};
use pgorm::{ActiveValue, DatabaseConnection, Error};

use super::metadata::EntityInfo;

pub(crate) type Model = Arc<dyn ModelBackend>;
pub(crate) type Active = Arc<dyn ActiveBackend>;
pub(crate) type Select = Arc<dyn SelectBackend>;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Comparison {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Terminal {
    All,
    One,
    Optional,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Write {
    Insert,
    Update,
    Delete,
}

#[derive(Clone, Debug)]
pub(crate) enum Change {
    Filter(Condition),
    Order(SimpleExpr, Order, Option<NullOrdering>),
    Limit(Option<u64>),
    Offset(Option<u64>),
}

pub(crate) trait EntityBackend: Debug + Send + Sync {
    fn info(&self) -> &Arc<EntityInfo>;
    fn select(&self) -> Select;
    fn active(&self) -> Active;
    fn expression(&self, column: &str) -> Result<SimpleExpr, Error>;
    fn compare(
        &self,
        column: &str,
        comparison: Comparison,
        value: Value,
    ) -> Result<SimpleExpr, Error>;
}

pub(crate) trait SelectBackend: Debug + Send + Sync {
    fn info(&self) -> &Arc<EntityInfo>;
    fn change(&self, change: Change) -> Select;
    fn compile(&self, terminal: Terminal) -> (String, Values);
    fn run<'a>(
        &'a self,
        db: &'a DatabaseConnection,
        terminal: Terminal,
    ) -> BoxFuture<'a, Result<Vec<Model>, Error>>;
}

pub(crate) trait ModelBackend: Debug + Send + Sync {
    fn info(&self) -> &Arc<EntityInfo>;
    fn get(&self, column: &str) -> Result<Value, Error>;
    fn set(&self, column: &str, value: Value) -> Result<Model, Error>;
    fn active(&self) -> Active;
}

pub(crate) enum Written {
    Model(Model),
    Count(u64),
}

pub(crate) trait ActiveBackend: Debug + Send + Sync {
    fn info(&self) -> &Arc<EntityInfo>;
    fn get(&self, column: &str) -> Result<ActiveValue<Value>, Error>;
    fn set(&self, column: &str, value: Value) -> Result<Active, Error>;
    fn not_set(&self, column: &str) -> Result<Active, Error>;
    fn reset(&self, column: &str) -> Result<Active, Error>;
    fn run<'a>(
        &'a self,
        db: &'a DatabaseConnection,
        write: Write,
    ) -> BoxFuture<'a, Result<Written, Error>>;
}
