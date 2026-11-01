//! Every SQL/JSON behaviour pgorm-query names is one a Python class spells,
//! so a behaviour pgorm-query gains fails this test until Python spells it.

use pgorm::pgorm_query::{
    JsonExistsBehavior, JsonQueryBehavior, JsonTableBehavior, JsonValueBehavior,
    tests_cfg::Variants,
};
use pyo3::{
    prelude::*,
    types::{PyDict, PyModule},
};

use super::{
    kinds::{PyJsonExistsBehavior, query_behavior, value_behavior},
    table::PyJsonTableBehavior,
};

/// Each member of the enum class `name`, as Python lists it.
fn members<'py>(module: &Bound<'py, PyModule>, name: &str) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let locals = PyDict::new(module.py());
    locals.set_item("cls", module.getattr(name)?)?;
    module
        .py()
        .eval(
            c"[member for member in vars(cls).values() if isinstance(member, cls)]",
            None,
            Some(&locals),
        )?
        .extract()
}

// [spec:pgorm:req:bindings.sql-json/test]
#[test]
fn every_sql_json_behaviour_has_a_spelling() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let module = PyModule::new(py, "pgorm")?;
        crate::values::register(&module)?;
        super::register(&module)?;
        let default = module.getattr("JsonDefault")?.call1((0,))?;
        let exists = members(&module, "JsonExistsBehavior")?
            .iter()
            .map(|member| Ok(member.extract::<PyJsonExistsBehavior>()?.rust()))
            .collect::<PyResult<Vec<_>>>()?;
        assert_eq!(JsonExistsBehavior::missing_from(&exists), [""; 0]);
        let value = members(&module, "JsonValueBehavior")?
            .iter()
            .chain([&default])
            .map(value_behavior)
            .collect::<PyResult<Vec<_>>>()?;
        assert_eq!(JsonValueBehavior::missing_from(&value), [""; 0]);
        let query = members(&module, "JsonQueryBehavior")?
            .iter()
            .chain([&default])
            .map(query_behavior)
            .collect::<PyResult<Vec<_>>>()?;
        assert_eq!(JsonQueryBehavior::missing_from(&query), [""; 0]);
        let table = members(&module, "JsonTableBehavior")?
            .iter()
            .map(|member| Ok(member.extract::<PyJsonTableBehavior>()?.rust()))
            .collect::<PyResult<Vec<_>>>()?;
        assert_eq!(JsonTableBehavior::missing_from(&table), [""; 0]);
        Ok(())
    })
}
