pub mod account;
pub mod graphs;
pub mod note;
mod registration;

use pyo3::prelude::*;

// Only registration belongs here: all conversions and execution are public pgorm.
// [spec:pgorm:req:generative.execution]
#[pymodule(gil_used = true)]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let mut registry = pgorm_python::entities::Registry::default();
    registration::register(&mut registry)?;
    pgorm_python::install(module, registry)
}
