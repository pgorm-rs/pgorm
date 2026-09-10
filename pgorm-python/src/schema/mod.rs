//! Explicit schema builders; construction never performs database work.

mod capabilities;
mod column;
mod entity;
mod enums;
mod index;
mod statement;
mod table;
mod types;

pub(crate) use capabilities::operations as capabilities;
pub use column::PyColumnDef;
pub use entity::PyEntitySchema;
pub use index::PyCreateIndex;
use pyo3::prelude::*;
pub use statement::PyDDL;
pub(crate) use statement::Statement;
pub use table::PyCreateTable;
pub use types::PyDataType;

pub(crate) fn install(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyDataType>()?;
    module.add_class::<PyColumnDef>()?;
    module.add_class::<PyCreateTable>()?;
    module.add_class::<PyCreateIndex>()?;
    module.add_class::<PyDDL>()?;
    module.add_class::<PyEntitySchema>()?;
    module.add_function(wrap_pyfunction!(entity::schema_from_entity, module)?)?;
    module.add_function(wrap_pyfunction!(enums::create_enum, module)?)?;
    module.add_function(wrap_pyfunction!(enums::add_enum_value, module)?)?;
    module.add_function(wrap_pyfunction!(enums::rename_enum_value, module)?)?;
    module.add_function(wrap_pyfunction!(enums::rename_enum, module)?)?;
    module.add_function(wrap_pyfunction!(enums::drop_enum, module)?)?;
    module.add_function(wrap_pyfunction!(index::drop_index, module)?)?;
    module.add_function(wrap_pyfunction!(table::drop_table, module)?)?;
    module.add_function(wrap_pyfunction!(table::rename_table, module)?)?;
    module.add_function(wrap_pyfunction!(table::rename_column, module)?)?;
    module.add_function(wrap_pyfunction!(table::truncate, module)?)?;
    module.add_function(wrap_pyfunction!(table::add_column, module)?)?;
    module.add_function(wrap_pyfunction!(table::modify_column, module)?)?;
    module.add_function(wrap_pyfunction!(table::drop_column, module)?)?;
    Ok(())
}
