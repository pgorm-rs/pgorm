//! Lossless native observations, in the shapes `pgorm_campaign.observations`
//! and `pgorm_campaign.effects` produce.
//!
//! Nothing here decides whether a result is *correct*. An observation records
//! what the subject saw, in a form the independent Python oracle can compare
//! against what it saw; a comparison performed on this side would be a second
//! oracle agreeing with the first.

use std::collections::HashSet;

use serde_json::{Value as Json, json};
use tokio_postgres::Row;

use crate::{FormatError, ObservedError, decode, wire::Tagged};

/// A set of rows.
///
/// # Errors
///
/// Returns [`FormatError`] when any column cannot be decoded losslessly.
pub fn rows(rows: &[Row]) -> Result<Json, FormatError> {
    Ok(json!({"kind": "rows", "rows": records(rows)?}))
}

/// A set of rows drained from a stream, with the stream's terminal state.
///
/// # Errors
///
/// Returns [`FormatError`] when any column cannot be decoded losslessly.
pub fn stream(
    rows: &[Row],
    complete: bool,
    cancelled: bool,
    closed: bool,
) -> Result<Json, FormatError> {
    Ok(json!({
        "kind": "rows",
        "rows": records(rows)?,
        "stream": {"complete": complete, "cancelled": cancelled, "closed": closed},
    }))
}

/// One row, with no owning entity.
///
/// # Errors
///
/// Returns [`FormatError`] when any column cannot be decoded losslessly, or
/// when two output columns share a name — an ambiguity the campaign resolves by
/// requiring every projection to carry a unique alias.
pub fn row(row: &Row) -> Result<Json, FormatError> {
    record(row, None)
}

/// One row, optionally naming the registered entity it was decoded into.
///
/// # Errors
///
/// Returns [`FormatError`] when any column cannot be decoded losslessly, or
/// when two output columns share a name.
pub fn record(row: &Row, entity: Option<&str>) -> Result<Json, FormatError> {
    let mut seen = HashSet::new();
    let mut fields = Vec::with_capacity(row.len());
    let mut postgres = Vec::with_capacity(row.len());
    for (index, column) in row.columns().iter().enumerate() {
        if !seen.insert(column.name()) {
            return Err(FormatError::new(
                "duplicate result field name; give each projection a unique alias",
            ));
        }
        let value = decode::value(row, index)?.encode_checked()?;
        fields.push(json!({"name": column.name(), "value": value}));
        postgres.push(json!({
            "name": column.name(),
            "type": column.type_().name(),
            "schema": column.type_().schema(),
        }));
    }
    let mut observation = json!({"kind": "record", "fields": fields, "postgres": postgres});
    if let (Some(entity), Some(object)) = (entity, observation.as_object_mut()) {
        object.insert("entity".to_owned(), json!(entity));
    }
    Ok(observation)
}

/// A terminal that yielded no row at all, as an optional graph slot does.
pub fn absent() -> Json {
    json!({"kind": "absent"})
}

/// A tuple of already-observed values, as a multi-source graph row is.
pub fn tuple(items: Vec<Json>) -> Json {
    json!({"kind": "tuple", "items": items})
}

/// An affected-row count.
///
/// The payload key is `value`, not `count`: `effects.py` spells it that way and
/// the Python report is what this one is compared against field by field.
pub fn count(count: u64) -> Json {
    json!({"kind": "count", "value": count})
}

/// An effect that produced no value, as a commit or rollback does.
pub fn unit() -> Json {
    json!({"kind": "unit"})
}

/// A transaction opened into a named scope.
pub fn transaction(scope: &str) -> Json {
    json!({"kind": "transaction", "scope": scope})
}

/// A built statement and the parameters bound to it, without executing either.
///
/// # Errors
///
/// Returns [`FormatError`] when a bound parameter is not a valid portable value.
pub fn compiled(sql: &str, parameters: &[Tagged]) -> Result<Json, FormatError> {
    let parameters = parameters
        .iter()
        .map(Tagged::encode_checked)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"kind": "compiled", "sql": sql, "parameters": parameters}))
}

/// A failure, classified the way the Python binding classifies the same cause.
pub fn error(error: &ObservedError) -> Json {
    json!({
        "kind": "error",
        "class": error.class(),
        "cause": error.cause(),
        "sqlstate": error.sqlstate(),
    })
}

fn records(rows: &[Row]) -> Result<Vec<Json>, FormatError> {
    rows.iter().map(row).collect()
}
