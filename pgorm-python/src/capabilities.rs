use pyo3::prelude::*;
use serde_json::{Value, json};

use crate::UnsupportedCapabilityError;

// [spec:pgorm:req:python.capabilities]
fn manifest() -> Value {
    json!({
        "schema_version": 1,
        "package_version": env!("CARGO_PKG_VERSION"),
        "pgorm_version": env!("PGORM_VERSION"),
        "binding": {"name": "pyo3", "version": "0.29.2", "registry_abi": 1},
        "target": env!("PGORM_BINDING_TARGET"),
        "features": ["macros", "with-json", "with-chrono", "with-uuid", "postgres-array", "runtime-tokio"],
        "transport": "in-process",
        "operations": {
            "pool": {"rust_api": "pgorm::connect_with", "features": ["runtime-tokio"]},
            "pool.acquire": {"rust_api": "pgorm::DatabasePool::get", "features": ["runtime-tokio"]},
            "pool.close": {"rust_api": "pgorm::DatabasePool::close", "features": ["runtime-tokio"]},
            "connection.ping": {"rust_api": "pgorm::ConnectionTrait::query_one", "features": ["runtime-tokio"]},
            "connection.close": {"rust_api": "pgorm::DatabaseConnection::drop", "features": ["runtime-tokio"]},
            "value": {"rust_api": "pgorm::pgorm_query::Value", "features": []},
            "value.array": {"rust_api": "pgorm::pgorm_query::Value::Array", "features": ["postgres-array"]},
            "value.json": {"rust_api": "pgorm::pgorm_query::Value::Json", "features": ["with-json"]},
            "value.null": {"rust_api": "pgorm::pgorm_query::Value", "features": []},
            "value.snapshot": {"rust_api": "pgorm_python::values::PyValue", "features": []},
            "type_name": {"rust_api": "pgorm::pgorm_query::TypeName", "features": []}
        },
        "value_types": crate::values::SCALAR_NAMES.iter().copied().chain(["enum", "array"]).collect::<Vec<_>>(),
        "value_policy": {
            "inferred_integer": "i64", "inferred_float": "f64", "array_dimensions": 1,
            "float32": "exact conversion only", "decimal": "96-bit coefficient, scale 0–28",
            "temporal_precision": "microseconds; leap seconds and subsecond offsets rejected",
            "aware_datetime": "explicit UTC, machine local, or fixed-offset tag",
            "enum_storage": "Rust String value with qualified TypeName metadata",
            "snapshot": "version 1, tagged JSON with integer strings and IEEE float bits"
        },
        "result_forms": ["pool", "connection", "bool"],
        "tls": {"modes": ["verify-full", "disable"], "default": "verify-full unless DSN explicitly disables TLS", "ca": "PEM or WebPKI roots"},
        "registrations": {"entities": [], "graphs": []},
        "python": {"abi": "cp314", "free_threading": false, "subinterpreters": false}
    })
}

/// Return a fresh, versioned description of this native build's public surface.
#[pyfunction]
pub(crate) fn capabilities(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    py.import("json")?
        .call_method1("loads", (manifest().to_string(),))
}

/// Fail explicitly when this build does not expose the requested operation.
#[pyfunction]
pub(crate) fn require_capability(operation: &str) -> PyResult<()> {
    if manifest()["operations"].get(operation).is_some() {
        Ok(())
    } else {
        Err(UnsupportedCapabilityError::new_err(format!(
            "this pgorm build does not support operation {operation:?}"
        )))
    }
}
