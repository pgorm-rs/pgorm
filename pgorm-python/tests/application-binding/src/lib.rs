//! Independent application crate: no pgorm repository fixture entities.

pub mod account;
pub mod graphs;
pub mod note;
pub mod sources;

#[cfg(test)]
mod graph_parity;
#[cfg(test)]
mod parity;

use pyo3::prelude::*;

// [spec:pgorm:req:python.entities/test]
#[pymodule(gil_used = true)]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let mut registry = pgorm_python::entities::Registry::default();
    registry.entity::<account::Entity>("app.Account")?;
    registry.entity::<note::Entity>("app.Note")?;
    graphs::register(&mut registry)?;
    sources::register(&mut registry)?;
    pgorm_python::install(module, registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    // [spec:pgorm:req:python.entities/test]
    #[test]
    fn registry_refuses_name_and_type_collisions() -> PyResult<()> {
        Python::initialize();
        let mut registry = pgorm_python::entities::Registry::default();
        registry.entity::<account::Entity>("app.Account")?;
        assert!(registry.entity::<note::Entity>("app.Account").is_err());
        assert!(
            registry
                .entity::<account::Entity>("duplicate.Account")
                .is_err()
        );
        registry.entity::<note::Entity>("app.Note")?;
        Python::attach(|py| {
            let module = PyModule::new(py, "pgorm._native")?;
            pgorm_python::install(&module, registry)?;
            assert!(pgorm_python::install(&module, Default::default()).is_err());
            let manifest = module.call_method0("capabilities")?;
            assert_eq!(
                manifest
                    .get_item("registrations")?
                    .get_item("entities")?
                    .len()?,
                2
            );
            Ok(())
        })
    }
}
