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
            "connection.close": {"rust_api": "pgorm::DatabaseConnection::drop", "features": ["runtime-tokio"]}
        },
        "value_types": [],
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
