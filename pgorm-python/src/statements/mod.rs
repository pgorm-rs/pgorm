//! Runtime statement construction through the native Rust query builders.

mod capabilities;
mod common;
mod conflict;
mod insert;
mod raw;
mod select;
mod table;
#[cfg(test)]
mod tests;
mod write;

pub(crate) use capabilities::operations as capabilities;
pub use conflict::{PyConflict, PyConflictTarget, PyConflictUpdate};
pub use insert::PyInsert;
use pyo3::prelude::*;
pub use raw::RawSQL;
pub use select::{Join, PySelect};
pub use table::PyTable;
pub use write::{PyDelete, PyUpdate};

/// Prepare the same validated builder state exposed by Python's inspection API.
pub fn compile(value: &Bound<'_, PyAny>) -> PyResult<crate::expressions::Compiled> {
    if let Ok(query) = value.extract::<PyRef<'_, PySelect>>() {
        query.inspect()
    } else if let Ok(query) = value.extract::<PyRef<'_, PyInsert>>() {
        query.inspect()
    } else if let Ok(query) = value.extract::<PyRef<'_, PyUpdate>>() {
        query.inspect()
    } else if let Ok(query) = value.extract::<PyRef<'_, PyDelete>>() {
        query.inspect()
    } else if let Ok(query) = value.extract::<PyRef<'_, RawSQL>>() {
        query.inspect()
    } else if let Ok(query) = value.extract::<PyRef<'_, crate::expressions::Compiled>>() {
        Ok(query.clone())
    } else if let Ok(query) = value.extract::<PyRef<'_, crate::pipeline::PyPipeline>>() {
        query.compile("all")
    } else {
        Err(crate::errors::ConstructionError::new_err(
            "execution requires a native statement, Compiled object or explicit RawSQL",
        ))
    }
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyTable>()?;
    module.add_class::<PySelect>()?;
    module.add_class::<Join>()?;
    module.add_class::<PyInsert>()?;
    module.add_class::<PyUpdate>()?;
    module.add_class::<PyDelete>()?;
    module.add_class::<PyConflict>()?;
    module.add_class::<PyConflictTarget>()?;
    module.add_class::<PyConflictUpdate>()?;
    module.add_class::<RawSQL>()?;
    Ok(())
}
