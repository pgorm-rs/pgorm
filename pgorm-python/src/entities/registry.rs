use std::{
    any::TypeId,
    collections::{BTreeMap, HashSet},
    marker::PhantomData,
    sync::Arc,
};

use pgorm::{EntityTrait, IntoActiveModel};
use pyo3::prelude::*;

use super::{PyEntity, backend::EntityBackend, generic::EntityAdapter, metadata::EntityInfo};
use crate::{UnsupportedCapabilityError, errors::ConstructionError};

/// Build-time collection of concrete Rust registrations for one native module.
/// A module owns the installed registry; Python cannot append generic types.
#[derive(Debug, Default)]
pub struct Registry {
    entries: BTreeMap<String, Arc<dyn EntityBackend>>,
    types: HashSet<TypeId>,
}

impl Registry {
    /// Register the real entity, model, columns and ActiveModel implementation.
    /// Duplicate names or duplicate Rust entity types are rejected atomically.
    // [spec:pgorm:req:python.entities]
    pub fn entity<E>(&mut self, name: &str) -> PyResult<&mut Self>
    where
        E: EntityTrait + Send + Sync + 'static,
        E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static,
        E::ActiveModel: Send + Sync + 'static,
    {
        if self.entries.contains_key(name) || self.types.contains(&TypeId::of::<E>()) {
            return Err(ConstructionError::new_err(
                "duplicate entity registration name or Rust type",
            ));
        }
        let info = Arc::new(EntityInfo::of::<E>(name)?);
        self.entries.insert(
            name.to_owned(),
            Arc::new(EntityAdapter::<E> {
                info,
                entity: PhantomData,
            }),
        );
        self.types.insert(TypeId::of::<E>());
        Ok(self)
    }

    pub(crate) fn describe(&self) -> Vec<serde_json::Value> {
        self.entries.values().map(|e| e.info().describe()).collect()
    }
}

#[derive(Debug)]
#[pyclass(name = "_Registry", module = "pgorm._native", frozen)]
pub(crate) struct NativeRegistry(pub(crate) Registry);

#[pyfunction(pass_module)]
pub(crate) fn entity(module: &Bound<'_, PyModule>, name: &str) -> PyResult<PyEntity> {
    let registry_object = module.getattr("_registry")?;
    let registry = registry_object.extract::<PyRef<'_, NativeRegistry>>()?;
    registry
        .0
        .entries
        .get(name)
        .cloned()
        .map(|backend| PyEntity { backend })
        .ok_or_else(|| {
            UnsupportedCapabilityError::new_err("entity is not registered in this native build")
        })
}
