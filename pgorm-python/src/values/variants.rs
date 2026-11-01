//! Every kind of pgorm value against Python's conversions, each of which ends
//! in an arm for a kind pgorm gained after this binding was written.

use pgorm::pgorm_query::{ArrayType, Value, tests_cfg::Variants};
use pyo3::{prelude::*, types::PyModule};

use super::{PyValue, convert, snapshot, unrepresented};

/// Whether `error` is the refusal of a kind pgorm-python does not represent:
/// the conversion's wildcard arm, which no kind pgorm names may reach.
fn unrepresented_kind(error: &PyErr) -> bool {
    error.to_string() == unrepresented().to_string()
}

// [spec:pgorm:req:bindings.values/test]
#[test]
fn no_value_kind_reaches_a_conversions_wildcard_arm() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let foreign = PyModule::import(py, "builtins")?
            .getattr("object")?
            .call0()?;
        for kind in ArrayType::variants().into_iter().chain(ArrayType::kinds()) {
            match convert::from_python(&foreign, &kind) {
                Ok(_) => panic!("{kind:?} converted a bare object() into a value"),
                Err(error) => assert!(!unrepresented_kind(&error), "{kind:?}: {error}"),
            }
            assert!(
                convert::to_python(py, &kind.null())?.is_none(py),
                "{kind:?}"
            );
        }
        for value in Value::variants() {
            let python = convert::to_python(py, &value)?;
            if !value.is_array() {
                let back = convert::from_python(python.bind(py), &value.array_type())?;
                assert_eq!(back, value);
            }
            snapshot::encode(&PyValue::from_rust(value))?;
        }
        Ok(())
    })
}
