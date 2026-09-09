//! Public native bindings for pgorm.
//!
//! The installed capability manifest records the implemented surface and the
//! concrete Rust API behind each operation. Downstream bindings share this crate.

mod capabilities;
mod config;
mod errors;
pub mod expressions;
pub mod identifiers;
pub mod results;
mod runtime;
pub mod statements;
pub mod values;

use pyo3::prelude::*;

pyo3::create_exception!(pgorm, PgOrmError, pyo3::exceptions::PyException);
pyo3::create_exception!(pgorm, UnsupportedCapabilityError, PgOrmError);

// [spec:pgorm:def:python.api]
// [spec:pgorm:req:python.package]
// [spec:pgorm:req:python.optional]
/// Initialize the native library without database or scanner side effects.
#[pymodule(gil_used = true)]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
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
    Ok(())
}
