//! Public native bindings for pgorm.
//!
//! The installed capability manifest records the implemented surface and the
//! concrete Rust API behind each operation. Downstream bindings share this crate.

mod capabilities;
mod config;
pub mod entities;
mod errors;
mod execution;
pub mod expressions;
pub mod graphs;
pub mod identifiers;
pub mod pipeline;
pub mod results;
mod runtime;
pub mod schema;
pub mod statements;
mod transactions;
pub mod values;

use pyo3::prelude::*;

pyo3::create_exception!(pgorm, PgOrmError, pyo3::exceptions::PyException);
pyo3::create_exception!(pgorm, UnsupportedCapabilityError, PgOrmError);

// [spec:pgorm:def:python.api]
// [spec:pgorm:req:python.package]
// [spec:pgorm:req:python.optional]
/// Initialize the native library without database or scanner side effects.
#[pymodule(gil_used = true)]
#[cfg(feature = "standalone-module")]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    install(module, entities::Registry::default())
}

/// Install the shared Python API and a concrete application registry into one
/// native module. Downstream crates disable `standalone-module` and export
/// their own PyO3 initializer calling this function.
// [spec:pgorm:req:python.entities]
pub fn install(module: &Bound<'_, PyModule>, registry: entities::Registry) -> PyResult<()> {
    if module.hasattr("_registry")? {
        return Err(errors::ConstructionError::new_err(
            "native module is already initialized",
        ));
    }
    let py = module.py();
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add("__pgorm_version__", env!("PGORM_VERSION"))?;
    module.add("PgOrmError", py.get_type::<PgOrmError>())?;
    module.add(
        "UnsupportedCapabilityError",
        py.get_type::<UnsupportedCapabilityError>(),
    )?;
    module.add_function(wrap_pyfunction!(capabilities::capabilities, module)?)?;
    module.add_function(wrap_pyfunction!(capabilities::require_capability, module)?)?;
    errors::register(module)?;
    runtime::register(module)?;
    values::register(module)?;
    expressions::register(module)?;
    identifiers::register(module)?;
    statements::register(module)?;
    results::register(module)?;
    entities::install(module, registry)?;
    graphs::install(module)?;
    pipeline::install(module)?;
    schema::install(module)?;
    Ok(())
}
