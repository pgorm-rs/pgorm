//! Registered Rust source tuples and their actual optional model decoders.

use crate::entities::{PyEntityModel, Registry, generic::ModelAdapter, metadata::EntityInfo};
use pgorm::{EntityTrait, IntoActiveModel, pgorm_query::Alias, pipeline as pl};
use pyo3::prelude::*;
use std::sync::Arc;

mod sealed {
    pub trait Sealed {}
}

/// Compiled entity identities for a concrete source tuple.
#[derive(Debug, Clone)]
pub struct SourceBindings {
    pub(crate) entities: Vec<Arc<EntityInfo>>,
}

/// A registered tuple of one to six concrete Rust entity types.
/// Each output position is optional, including the first under right/full joins.
// [spec:pgorm:req:python.pipeline]
pub trait SourceTypes: sealed::Sealed + Send + Sync + 'static {
    #[doc(hidden)]
    type Selection: pl::SourceList + Send + Sync + 'static;
    #[doc(hidden)]
    fn bindings(registry: &Registry) -> PyResult<SourceBindings>;
    #[doc(hidden)]
    fn select(
        pipeline: pl::Pipeline,
        qualifiers: &[String],
    ) -> pl::SelectedSources<Self::Selection>;
    #[doc(hidden)]
    fn models(
        row: <Self::Selection as pl::SourceList>::Row,
        bindings: &SourceBindings,
    ) -> Vec<Option<PyEntityModel>>;
}

fn model<E>(value: Option<E::Model>, info: &Arc<EntityInfo>) -> Option<PyEntityModel>
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    value.map(|value| PyEntityModel {
        inner: Arc::new(ModelAdapter::<E> {
            value,
            info: info.clone(),
        }),
    })
}

impl<E> sealed::Sealed for (E,) {}
impl<E> SourceTypes for (E,)
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    type Selection = pl::Named<E>;
    fn bindings(registry: &Registry) -> PyResult<SourceBindings> {
        Ok(SourceBindings {
            entities: vec![registry.registered::<E>()?],
        })
    }
    fn select(
        pipeline: pl::Pipeline,
        qualifiers: &[String],
    ) -> pl::SelectedSources<Self::Selection> {
        pipeline.select_sources(pl::named_runtime(E::default(), Alias::new(&qualifiers[0])))
    }
    fn models(row: Option<E::Model>, bindings: &SourceBindings) -> Vec<Option<PyEntityModel>> {
        vec![model::<E>(row, &bindings.entities[0])]
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
            fn bindings(registry: &Registry) -> PyResult<SourceBindings> {
                Ok(SourceBindings {entities: vec![$(registry.registered::<$entity>()?,)+]})
            }
            fn select(pipeline: pl::Pipeline, qualifiers: &[String]) -> pl::SelectedSources<Self::Selection> {
                pipeline.select_sources(($(pl::named_runtime($entity::default(), Alias::new(&qualifiers[$index])),)+))
            }
            fn models(row: <Self::Selection as pl::SourceList>::Row, bindings: &SourceBindings) -> Vec<Option<PyEntityModel>> {
                vec![$(model::<$entity>(row.$index, &bindings.entities[$index]),)+]
            }
        }
    };
}
sources!(E1 @ 0, E2 @ 1);
sources!(E1 @ 0, E2 @ 1, E3 @ 2);
sources!(E1 @ 0, E2 @ 1, E3 @ 2, E4 @ 3);
sources!(E1 @ 0, E2 @ 1, E3 @ 2, E4 @ 3, E5 @ 4);
sources!(E1 @ 0, E2 @ 1, E3 @ 2, E4 @ 3, E5 @ 4, E6 @ 5);
