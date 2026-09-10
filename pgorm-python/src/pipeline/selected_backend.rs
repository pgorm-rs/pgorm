use super::slots::{SourceBindings, SourceTypes};
use crate::execution::Database;
use crate::{entities::PyEntityModel, errors::ConstructionError, expressions::Compiled};
use futures_util::future::BoxFuture;
use pgorm::{
    Error,
    pipeline::{self as pl, SourceList},
};
use pyo3::prelude::*;
use std::{fmt::Debug, marker::PhantomData, sync::Arc};

pub(crate) type Row = Vec<Option<PyEntityModel>>;
pub(crate) type Query = Arc<dyn QueryBackend>;

#[derive(Debug, Clone)]
pub(crate) struct Info {
    pub(crate) name: String,
    pub(crate) rust_shape: &'static str,
    pub(crate) bindings: SourceBindings,
}

impl Info {
    pub(crate) fn describe(&self) -> serde_json::Value {
        serde_json::json!({"name": self.name, "rust_shape": self.rust_shape,
            "entities": self.bindings.entities.iter().map(|e| &e.name).collect::<Vec<_>>(),
            "terminals": ["all", "one", "one_opt"], "result": "tuple of optional models in source order"})
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Terminal {
    All,
    One,
    Optional,
}
impl Terminal {
    pub(crate) fn parse(value: &str) -> PyResult<Self> {
        match value {
            "all" => Ok(Self::All),
            "one" => Ok(Self::One),
            "one_opt" => Ok(Self::Optional),
            _ => Err(ConstructionError::new_err("unknown pipeline terminal")),
        }
    }
}

pub(crate) trait Factory: Debug + Send + Sync {
    fn info(&self) -> &Arc<Info>;
    fn select(&self, pipeline: pl::Pipeline, qualifiers: Vec<String>) -> Query;
}

pub(crate) trait QueryBackend: Debug + Send + Sync {
    fn compile(&self, terminal: Terminal) -> PyResult<Compiled>;
    fn run<'a>(
        &'a self,
        db: Database<'a>,
        terminal: Terminal,
    ) -> BoxFuture<'a, Result<Vec<Row>, Error>>;
}

pub(crate) struct SourceFactory<T> {
    pub(crate) info: Arc<Info>,
    pub(crate) marker: PhantomData<T>,
}

impl<T> Debug for SourceFactory<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceFactory")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

impl<T: SourceTypes> Factory for SourceFactory<T>
where
    <T::Selection as SourceList>::Row: Send,
{
    fn info(&self) -> &Arc<Info> {
        &self.info
    }
    fn select(&self, pipeline: pl::Pipeline, qualifiers: Vec<String>) -> Query {
        Arc::new(SourceQuery::<T> {
            pipeline,
            qualifiers,
            info: self.info.clone(),
            marker: PhantomData,
        })
    }
}

struct SourceQuery<T> {
    pipeline: pl::Pipeline,
    qualifiers: Vec<String>,
    info: Arc<Info>,
    marker: PhantomData<T>,
}
impl<T> Debug for SourceQuery<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceQuery")
            .field("info", &self.info)
            .field("qualifiers", &self.qualifiers)
            .finish_non_exhaustive()
    }
}

impl<T: SourceTypes> SourceQuery<T> {
    async fn read(&self, db: Database<'_>, terminal: Terminal) -> Result<Vec<Row>, Error> {
        let query = T::select(self.pipeline.clone(), &self.qualifiers);
        let models = |row| T::models(row, &self.info.bindings);
        match terminal {
            Terminal::All => Ok(query.all(&db).await?.into_iter().map(models).collect()),
            Terminal::One => Ok(vec![models(query.one(&db).await?)]),
            Terminal::Optional => Ok(query.one_opt(&db).await?.map(models).into_iter().collect()),
        }
    }
}

// [spec:pgorm:req:python.pipeline]
impl<T: SourceTypes> QueryBackend for SourceQuery<T>
where
    <T::Selection as SourceList>::Row: Send,
{
    fn compile(&self, terminal: Terminal) -> PyResult<Compiled> {
        let pipeline = match terminal {
            Terminal::All => self.pipeline.clone(),
            _ => self.pipeline.clone().take(1),
        };
        let (sql, values) = T::select(pipeline, &self.qualifiers)
            .into_sql()
            .map_err(|error| ConstructionError::new_err(error.to_string()))?;
        if values.0.len() > 65535 {
            return Err(ConstructionError::new_err(
                "PostgreSQL supports at most 65535 query parameters",
            ));
        }
        Ok(Compiled { sql, values })
    }
    fn run<'a>(
        &'a self,
        db: Database<'a>,
        terminal: Terminal,
    ) -> BoxFuture<'a, Result<Vec<Row>, Error>> {
        Box::pin(self.read(db, terminal))
    }
}
