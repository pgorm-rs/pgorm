//! Registered source tuples for a pipeline's `select_sources`: one to six
//! concrete entity types fixed in Rust, which a JavaScript pipeline's last
//! stage projects under qualifiers of its choosing and whose rows Rust decodes
//! through each entity's absence witness, every position optional.

use std::{fmt, marker::PhantomData, sync::Arc};

use futures_util::future::BoxFuture;
use pgorm::{
    EntityTrait, Error, IntoActiveModel,
    pgorm_query::{Name, Values},
    pipeline::{self as pl, PipelineError, SourceList},
};

use super::{
    Registry,
    adapter::{Model, model_of},
    info::{EntityInfo, registration_name},
    registry::RegistrationError,
};
use crate::connect::job::Database;

mod sealed {
    pub trait Sealed {}
}

/// The registered entities of a source tuple, in order.
#[derive(Clone, Debug)]
pub struct SourceBindings {
    pub(crate) entities: Vec<Arc<EntityInfo>>,
}

/// One decoded model of a selected row, behind its type-erased handle.
#[derive(Clone, Debug)]
pub struct SourceModel(pub(crate) Model);

/// A tuple of one to six concrete entity types a pipeline's rows decode as.
/// pgorm's own `SourceList` fixes the arities; this is sealed the same way.
// [spec:pgorm:req:napi.pipeline-sources]
pub trait SourceTypes: sealed::Sealed + Send + Sync + 'static {
    #[doc(hidden)]
    type Selection: SourceList + Send + Sync + 'static;
    #[doc(hidden)]
    fn bindings(registry: &Registry) -> Result<SourceBindings, RegistrationError>;
    #[doc(hidden)]
    fn select(
        pipeline: pl::Pipeline,
        qualifiers: &[String],
    ) -> pl::SelectedSources<Self::Selection>;
    #[doc(hidden)]
    fn models(
        row: <Self::Selection as SourceList>::Row,
        bindings: &SourceBindings,
    ) -> Vec<Option<SourceModel>>;
}

fn decoded<E>(value: Option<E::Model>, bindings: &SourceBindings, at: usize) -> Option<SourceModel>
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    let info = bindings.entities.get(at)?;
    value.map(|value| SourceModel(model_of::<E>(value, info)))
}

impl<E> sealed::Sealed for (E,) {}

impl<E> SourceTypes for (E,)
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    type Selection = pl::Named<E>;

    fn bindings(registry: &Registry) -> Result<SourceBindings, RegistrationError> {
        Ok(SourceBindings {
            entities: vec![registry.registered::<E>()?],
        })
    }

    fn select(
        pipeline: pl::Pipeline,
        qualifiers: &[String],
    ) -> pl::SelectedSources<Self::Selection> {
        let qualifier = qualifiers.first().map_or("", String::as_str);
        pipeline.select_sources(pl::named_runtime(E::default(), Name::runtime(qualifier)))
    }

    fn models(row: Option<E::Model>, bindings: &SourceBindings) -> Vec<Option<SourceModel>> {
        vec![decoded::<E>(row, bindings, 0)]
    }
}

macro_rules! sources {
    ($($entity:ident @ $index:tt),+) => {
        impl<$($entity),+> sealed::Sealed for ($($entity,)+) {}

        impl<$($entity),+> SourceTypes for ($($entity,)+)
        where $(
            $entity: EntityTrait + Send + Sync + 'static,
            $entity::Model: IntoActiveModel<$entity::ActiveModel> + Sync + 'static,
            $entity::ActiveModel: Send + Sync + 'static,
        )+ {
            type Selection = ($(pl::Named<$entity>,)+);

            fn bindings(registry: &Registry) -> Result<SourceBindings, RegistrationError> {
                Ok(SourceBindings { entities: vec![$(registry.registered::<$entity>()?,)+] })
            }

            fn select(pipeline: pl::Pipeline, qualifiers: &[String]) -> pl::SelectedSources<Self::Selection> {
                let qualifier = |at: usize| Name::runtime(qualifiers.get(at).map_or("", String::as_str));
                pipeline.select_sources(($(pl::named_runtime($entity::default(), qualifier($index)),)+))
            }

            fn models(
                row: <Self::Selection as SourceList>::Row,
                bindings: &SourceBindings,
            ) -> Vec<Option<SourceModel>> {
                vec![$(decoded::<$entity>(row.$index, bindings, $index),)+]
            }
        }
    };
}

sources!(E1 @ 0, E2 @ 1);
sources!(E1 @ 0, E2 @ 1, E3 @ 2);
sources!(E1 @ 0, E2 @ 1, E3 @ 2, E4 @ 3);
sources!(E1 @ 0, E2 @ 1, E3 @ 2, E4 @ 3, E5 @ 4);
sources!(E1 @ 0, E2 @ 1, E3 @ 2, E4 @ 3, E5 @ 4, E6 @ 5);

/// A registered source tuple: its name, the Rust shape and its entities.
#[derive(Debug)]
pub(crate) struct SourcesInfo {
    pub(crate) name: String,
    pub(crate) shape: &'static str,
    pub(crate) bindings: SourceBindings,
}

impl SourcesInfo {
    pub(crate) fn describe(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "rustShape": self.shape,
            "entities": self.bindings.entities.iter().map(|entity| &entity.name).collect::<Vec<_>>(),
        })
    }
}

pub(crate) type Selected = Arc<dyn SelectedBackend>;
pub(crate) type Row = Vec<Option<SourceModel>>;

pub(crate) trait SourcesFactory: fmt::Debug + Send + Sync {
    fn info(&self) -> &Arc<SourcesInfo>;
    fn select(&self, pipeline: pl::Pipeline, qualifiers: Vec<String>) -> Selected;
}

pub(crate) trait SelectedBackend: fmt::Debug + Send + Sync {
    fn info(&self) -> &Arc<SourcesInfo>;
    fn compile(&self, take_one: bool) -> Result<(String, Values), PipelineError>;
    fn run<'a>(
        &'a self,
        db: Database<'a>,
        read: super::adapter::Read,
    ) -> BoxFuture<'a, Result<Vec<Row>, Error>>;
}

pub(crate) struct Factory<T> {
    pub(crate) info: Arc<SourcesInfo>,
    pub(crate) marker: PhantomData<T>,
}

impl<T> fmt::Debug for Factory<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Factory")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

impl<T: SourceTypes> SourcesFactory for Factory<T>
where
    <T::Selection as SourceList>::Row: Send,
{
    fn info(&self) -> &Arc<SourcesInfo> {
        &self.info
    }

    fn select(&self, pipeline: pl::Pipeline, qualifiers: Vec<String>) -> Selected {
        Arc::new(Query::<T> {
            pipeline,
            qualifiers,
            info: self.info.clone(),
            marker: PhantomData,
        })
    }
}

struct Query<T> {
    pipeline: pl::Pipeline,
    qualifiers: Vec<String>,
    info: Arc<SourcesInfo>,
    marker: PhantomData<T>,
}

impl<T> fmt::Debug for Query<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Query")
            .field("info", &self.info)
            .field("qualifiers", &self.qualifiers)
            .finish_non_exhaustive()
    }
}

// [spec:pgorm:req:napi.pipeline-sources]
impl<T: SourceTypes> SelectedBackend for Query<T>
where
    <T::Selection as SourceList>::Row: Send,
{
    fn info(&self) -> &Arc<SourcesInfo> {
        &self.info
    }

    fn compile(&self, take_one: bool) -> Result<(String, Values), PipelineError> {
        let pipeline = if take_one {
            self.pipeline.clone().take(1)
        } else {
            self.pipeline.clone()
        };
        T::select(pipeline, &self.qualifiers).into_sql()
    }

    fn run<'a>(
        &'a self,
        db: Database<'a>,
        read: super::adapter::Read,
    ) -> BoxFuture<'a, Result<Vec<Row>, Error>> {
        Box::pin(async move {
            let query = T::select(self.pipeline.clone(), &self.qualifiers);
            let bindings = &self.info.bindings;
            Ok(match read {
                super::adapter::Read::All => query
                    .all(&db)
                    .await?
                    .into_iter()
                    .map(|row| T::models(row, bindings))
                    .collect(),
                super::adapter::Read::One => vec![T::models(query.one(&db).await?, bindings)],
                super::adapter::Read::OneOpt => query
                    .one_opt(&db)
                    .await?
                    .map(|row| T::models(row, bindings))
                    .into_iter()
                    .collect(),
            })
        })
    }
}

impl Registry {
    /// Register a tuple of one to six entity types a pipeline's
    /// `select_sources` decodes rows as, each entity registered first.
    // [spec:pgorm:req:napi.pipeline-sources]
    pub fn sources<T: SourceTypes>(&mut self, name: &str) -> Result<&mut Self, RegistrationError>
    where
        <T::Selection as SourceList>::Row: Send,
    {
        registration_name(name)?;
        if self.sources.contains_key(name) {
            return Err(RegistrationError(format!(
                "the source tuple {name:?} is already registered"
            )));
        }
        let info = Arc::new(SourcesInfo {
            name: name.to_owned(),
            shape: std::any::type_name::<T>(),
            bindings: T::bindings(self)?,
        });
        self.sources.insert(
            name.to_owned(),
            Arc::new(Factory::<T> {
                info,
                marker: PhantomData,
            }),
        );
        Ok(self)
    }
}
