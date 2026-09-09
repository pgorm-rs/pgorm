use std::sync::Arc;

use pgorm::{
    EntityTrait, GraphItem, GraphRow, IntoActiveModel, Opt, Req, SelectorTrait, Slot, Slots,
};
use pyo3::prelude::*;

use crate::entities::{PyEntityModel, Registry, generic::ModelAdapter, metadata::EntityInfo};

#[derive(Clone, Debug)]
pub(crate) struct Source {
    pub(crate) entity: Arc<EntityInfo>,
    pub(crate) optional: bool,
}

/// The registered entity identities of a concrete Rust graph tuple.
/// Produced by GraphSlots; applications do not construct this value.
#[derive(Clone, Debug)]
pub struct GraphBindings {
    pub(crate) sources: Vec<Source>,
}

pub(crate) trait RegisteredSlot: Slot {
    fn source(registry: &Registry) -> PyResult<Source>;
    fn model(value: Self::Out, source: &Source) -> Option<PyEntityModel>;
}

impl<F> RegisteredSlot for Req<F>
where
    F: EntityTrait + Send + Sync + 'static,
    F::Model: IntoActiveModel<F::ActiveModel> + Sync + 'static,
    F::ActiveModel: Send + Sync + 'static,
{
    fn source(registry: &Registry) -> PyResult<Source> {
        Ok(Source {
            entity: registry.registered::<F>()?,
            optional: false,
        })
    }
    fn model(value: Self::Out, source: &Source) -> Option<PyEntityModel> {
        Some(PyEntityModel {
            inner: Arc::new(ModelAdapter::<F> {
                value,
                info: source.entity.clone(),
            }),
        })
    }
}

impl<F> RegisteredSlot for Opt<F>
where
    F: EntityTrait + Send + Sync + 'static,
    F::Model: IntoActiveModel<F::ActiveModel> + Sync + 'static,
    F::ActiveModel: Send + Sync + 'static,
{
    fn source(registry: &Registry) -> PyResult<Source> {
        Ok(Source {
            entity: registry.registered::<F>()?,
            optional: true,
        })
    }
    fn model(value: Self::Out, source: &Source) -> Option<PyEntityModel> {
        value.map(|value| PyEntityModel {
            inner: Arc::new(ModelAdapter::<F> {
                value,
                info: source.entity.clone(),
            }),
        })
    }
}

/// Native conversion machinery for Rust's existing graph tuple arities.
/// The pgorm Slots supertrait is sealed; arbitrary shapes cannot implement it.
pub trait GraphSlots<E: EntityTrait>:
    Slots + std::fmt::Debug + Send + Sync + Sized + 'static
where
    GraphRow<E, Self>: SelectorTrait,
{
    #[doc(hidden)]
    fn bindings(registry: &Registry) -> PyResult<GraphBindings>;
    #[doc(hidden)]
    fn models(row: GraphItem<E, Self>, bindings: &GraphBindings) -> Vec<Option<PyEntityModel>>;
}

// [spec:pgorm:req:python.graph]
impl<E> GraphSlots<E> for ()
where
    E: EntityTrait + Send + Sync + 'static,
    E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
    E::ActiveModel: Send + Sync + 'static,
{
    fn bindings(registry: &Registry) -> PyResult<GraphBindings> {
        Ok(GraphBindings {
            sources: vec![Source {
                entity: registry.registered::<E>()?,
                optional: false,
            }],
        })
    }
    fn models(value: E::Model, bindings: &GraphBindings) -> Vec<Option<PyEntityModel>> {
        vec![Some(PyEntityModel {
            inner: Arc::new(ModelAdapter::<E> {
                value,
                info: bindings.sources[0].entity.clone(),
            }),
        })]
    }
}

macro_rules! slots {
    ($($slot:ident @ $at:tt),+) => {
        // [spec:pgorm:req:python.graph]
        impl<E, $($slot),+> GraphSlots<E> for ($($slot,)+)
        where
            E: EntityTrait + Send + Sync + 'static,
            E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
            E::ActiveModel: Send + Sync + 'static,
            $($slot: RegisteredSlot + std::fmt::Debug + Send + Sync + 'static,)+
        {
            fn bindings(registry: &Registry) -> PyResult<GraphBindings> {
                Ok(GraphBindings { sources: vec![Source { entity: registry.registered::<E>()?, optional: false }, $($slot::source(registry)?,)+] })
            }
            fn models(row: GraphItem<E, Self>, bindings: &GraphBindings) -> Vec<Option<PyEntityModel>> {
                let root = PyEntityModel { inner: Arc::new(ModelAdapter::<E> { value: row.0, info: bindings.sources[0].entity.clone() }) };
                vec![Some(root), $($slot::model(row.$at, &bindings.sources[$at]),)+]
            }
        }
    }
}

slots!(S1 @ 1);
slots!(S1 @ 1, S2 @ 2);
slots!(S1 @ 1, S2 @ 2, S3 @ 3);
slots!(S1 @ 1, S2 @ 2, S3 @ 3, S4 @ 4);
slots!(S1 @ 1, S2 @ 2, S3 @ 3, S4 @ 4, S5 @ 5);
slots!(S1 @ 1, S2 @ 2, S3 @ 3, S4 @ 4, S5 @ 5, S6 @ 6);
