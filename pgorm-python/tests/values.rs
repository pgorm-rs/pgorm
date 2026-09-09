use std::ffi::CString;

use chrono::{NaiveDate, NaiveTime};
use pgorm::pgorm_query::{ArrayType, Value};
use pgorm_python::values::{PyTypeName, PyValue};
use pyo3::prelude::*;

// [spec:pgorm:req:python.values/test]
#[test]
fn python_scalars_preserve_rust_variants() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let cases = [
            ("bool", "True", Value::Bool(Some(true))),
            ("i8", "-128", Value::TinyInt(Some(-128))),
            ("i16", "32767", Value::SmallInt(Some(32767))),
            ("i32", "2147483647", Value::Int(Some(i32::MAX))),
            ("i64", "-9223372036854775808", Value::BigInt(Some(i64::MIN))),
            ("u32", "4294967295", Value::Unsigned(Some(u32::MAX))),
            (
                "u64",
                "18446744073709551615",
                Value::BigUnsigned(Some(u64::MAX)),
            ),
            ("f32", "-0.0", Value::Float(Some(-0.0))),
            ("f64", "0.1", Value::Double(Some(0.1))),
            (
                "text",
                "'雪%\\\\'",
                Value::String(Some(Box::new("雪%\\".into()))),
            ),
            ("char", "'雪'", Value::Char(Some('雪'))),
            (
                "bytes",
                "b'\\x00\\xff'",
                Value::Bytes(Some(Box::new(vec![0, 255]))),
            ),
        ];
        for (kind, expression, expected) in cases {
            let expression = CString::new(expression)?;
            let input = py.eval(&expression, None, None)?;
            let actual = py.get_type::<PyValue>().call1((input, kind))?;
            assert_eq!(
                actual.extract::<PyRef<'_, PyValue>>()?.rust_value(),
                &expected
            );
            let inverse = Py::new(py, PyValue::from_rust(expected))?;
            assert!(actual.eq(inverse)?);
        }
        Ok(())
    })
}

// [spec:pgorm:req:python.value-tags/test]
#[test]
fn rust_output_rejects_lost_temporal_precision() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let submicro = NaiveTime::from_hms_nano_opt(12, 0, 0, 123_456_789)
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid fixture"))?;
        let leap = NaiveTime::from_hms_nano_opt(23, 59, 59, 1_000_000_000)
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid fixture"))?;
        let old = NaiveDate::from_ymd_opt(0, 1, 1)
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid fixture"))?;
        for inner in [
            Value::ChronoTime(Some(Box::new(submicro))),
            Value::ChronoTime(Some(Box::new(leap))),
            Value::ChronoDate(Some(Box::new(old))),
            Value::Float(Some(f32::from_bits(0x7f800001))),
        ] {
            let value = Py::new(py, PyValue::from_rust(inner))?;
            let error = value.bind(py).getattr("value").err().ok_or_else(|| {
                pyo3::exceptions::PyAssertionError::new_err("lossy conversion succeeded")
            })?;
            assert_eq!(error.get_type(py).name()?, "DecodeError");
            // Inspection still preserves the original Rust payload for diagnosis.
            value.bind(py).call_method0("snapshot")?;
        }
        Ok(())
    })
}

// [spec:pgorm:req:python.values/test]
// [spec:pgorm:req:python.value-tags/test]
#[test]
fn arrays_and_json_null_keep_rust_identity() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let class = py.get_type::<PyValue>();
        let json_null = class.call_method1("json", (py.None(),))?;
        assert_eq!(
            json_null.extract::<PyRef<'_, PyValue>>()?.rust_value(),
            &Value::Json(Some(Box::new(serde_json::Value::Null)))
        );
        let null = class.call_method1("null", ("json",))?;
        assert_eq!(
            null.extract::<PyRef<'_, PyValue>>()?.rust_value(),
            &Value::Json(None)
        );
        let input = vec![Some(7i32), None];
        let array = class.call_method1("array", ("i32", input))?;
        assert_eq!(
            array.extract::<PyRef<'_, PyValue>>()?.rust_value(),
            &Value::Array(
                ArrayType::Int,
                Some(Box::new(vec![Value::Int(Some(7)), Value::Int(None)]))
            )
        );
        let name = py.get_type::<PyTypeName>().call1(("Mood\"雪",))?;
        let enum_value = class.call1(("calm", &name))?;
        assert_eq!(
            enum_value.extract::<PyRef<'_, PyValue>>()?.rust_value(),
            &Value::String(Some(Box::new("calm".into())))
        );
        let rust_type = name.extract::<PyRef<'_, PyTypeName>>()?.rust_type();
        assert!(!rust_type.verbatim);
        assert_eq!(rust_type.name.to_string(), "Mood\"雪");
        Ok(())
    })
}
