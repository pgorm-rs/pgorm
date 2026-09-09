use pgorm::pgorm_query::Values;
use pyo3::prelude::*;

use crate::values::PyValue;

/// A Rust-built SQL statement and its independently owned, ordered parameters.
#[pyclass(name = "Compiled", module = "pgorm", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct Compiled {
    #[pyo3(get)]
    pub sql: String,
    pub values: Values,
}

#[pymethods]
impl Compiled {
    #[getter]
    fn params(&self) -> Vec<PyValue> {
        self.values
            .0
            .iter()
            .cloned()
            .map(PyValue::from_rust)
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("Compiled(parameters={})", self.values.0.len())
    }
}
