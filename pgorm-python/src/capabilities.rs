use pyo3::prelude::*;
use serde_json::{Value, json};

use crate::UnsupportedCapabilityError;

// [spec:pgorm:req:python.capabilities]
fn manifest() -> Value {
    let mut manifest = json!({
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
        "expression_functions": {
            "lower": [1], "upper": [1], "abs": [1], "char_length": [1],
            "count": [1], "count_distinct": [1], "sum": [1], "avg": [1],
            "min": [1], "max": [1], "round": [1, 2], "coalesce": {"min_args": 1},
            "random": [0], "gen_random_uuid": [0]
        },
        "result_forms": ["pool", "connection", "bool", "expression", "condition", "compiled", "projection", "ordering", "record", "optional_record", "records", "affected_count", "async_stream", "entity", "entity_query", "entity_model", "active_model", "active_value", "graph", "graph_query", "graph_cursor", "graph_tuple"],
        "result_policy": {
            "scope": "dynamic Record results",
            "decode": "Rust Row::try_get / FromSql, Value conversion with exactness checks",
            "names": "unique output names required; explicit aliases resolve duplicates",
            "timestamptz": "UTC; PostgreSQL does not retain the input timezone",
            "numeric": "exact 96-bit Decimal coefficient and scale 0–28; other values rejected",
            "json": "serde_json i64/u64/f64; numeric value loss and nesting beyond 64 rejected",
            "arrays": "one dimension, lower bound 1, nullable elements; other shapes rejected",
            "unsupported": ["domain", "composite", "range", "multirange", "interval", "timetz", "bit", "money"],
            "stream": "one pull at a time over bounded driver buffers; owns connection until EOF or close"
        },
        "entity_policy": {
            "registration": "concrete Rust types compiled into the same native module",
            "decode": "registered Model::from_query_result followed by checked Python Value conversion",
            "input": "SQL ColumnType hints for plain values; explicit Value tags and Rust setters remain authoritative",
            "one": "Rust Select::one/one_opt use LIMIT 1; all returns every selected model",
            "writes": "real ActiveModel insert/update/delete and before/after hooks on a clone",
            "execution": "acquired Connection; native asyncio awaitable",
            "stream": false
        },
        "graph_policy": {
            "source_arities": [1, 2, 3, 4, 5, 6, 7],
            "slots": ["Req", "Opt"], "registration_required": true,
            "decode": "SelectGraph / GraphRow with actual registered models and optional slots",
            "result": "bare model for one source; tuple in source order for joined shapes",
            "cursor": "one root column plus Rust's declared primary-key tiebreaks",
            "unsupported": ["unregistered shapes", "source arity beyond seven", "graph.stream", "graph.grouped", "slot-column cursors", "composite order-column cursors", "cursor SQL inspection"]
        },
        "codegen_policy": {
            "workflow": ["pgorm.codegen scaffold", "native application build and install", "pgorm.codegen emit", "final wheel build and install"],
            "input_schema_version": 1,
            "typing_source": "FromQueryResult::expected_columns Rust field spellings; unknown custom types remain Any",
            "compatibility": ["package_version", "pgorm_version", "registry_abi", "features", "compiled entity and graph metadata"],
            "python_conveniences": {
                "model properties": "EntityModel lookup over ModelTrait::get with checked Value conversion",
                "set_<field>": "typed Value construction then ActiveModelTrait::set on a cloned native model",
                "model and active views": "registered native EntityModel and ActiveModel methods",
                "entity query views": "registered native EntityQuery methods over Select<Entity>",
                "graph and cursor views": "registered native GraphQuery and GraphCursor methods over SelectGraph; wrap decoded models in concrete Python views"
            }
        },
        "tls": {"modes": ["verify-full", "disable"], "default": "verify-full unless DSN explicitly disables TLS", "ca": "PEM or WebPKI roots"},
        "registrations": {"entities": [], "graphs": []},
        "python": {"abi": "cp314", "free_threading": false, "subinterpreters": false}
    });
    if let Some(operations) = manifest["operations"].as_object_mut() {
        operations.extend(crate::expressions::capabilities());
        operations.extend(crate::statements::capabilities());
        operations.extend(crate::results::capabilities());
        operations.extend(crate::entities::capabilities());
        operations.extend(crate::graphs::capabilities());
    }
    manifest
}

/// Return a fresh, versioned description of this native build's public surface.
#[pyfunction(pass_module)]
pub(crate) fn capabilities<'py>(module: &Bound<'py, PyModule>) -> PyResult<Bound<'py, PyAny>> {
    let mut manifest = manifest();
    let registry_object = module.getattr("_registry")?;
    let registry = registry_object.extract::<PyRef<'_, crate::entities::NativeRegistry>>()?;
    manifest["registrations"]["entities"] = registry.0.describe().into();
    manifest["registrations"]["graphs"] = registry
        .0
        .graphs
        .values()
        .map(|graph| graph.info().describe())
        .collect::<Vec<_>>()
        .into();
    module
        .py()
        .import("json")?
        .call_method1("loads", (manifest.to_string(),))
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
