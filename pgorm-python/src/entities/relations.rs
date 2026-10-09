//! What `describe()` reports of a registered entity's relations: the parts of
//! a compiled declaration that change what a join means, though no Python
//! call names them.

use pgorm::pgorm_query::{Deferrability, Enforcement, FromItem};
use pgorm::{EntityTrait, Iterable, RelationDef, RelationTrait, RelationType};
use serde_json::{Value as Json, json};

// [spec:pgorm:req:python.entities+2]
/// Every relation the entity's `Relation` enum declares, in declaration order.
pub(super) fn describe<E: EntityTrait>() -> Vec<Json> {
    E::Relation::iter()
        .map(|relation| relation_json(&format!("{relation:?}"), &relation.def()))
        .collect()
}

fn relation_json(name: &str, def: &RelationDef) -> Json {
    json!({
        "name": name,
        "type": match def.rel_type {
            RelationType::HasOne => "has_one",
            RelationType::HasMany => "has_many",
        },
        "from": table(&def.from_tbl),
        "to": table(&def.to_tbl),
        "columns": def
            .columns
            .iter()
            .map(|(from, to)| [from.to_string(), to.to_string()])
            .collect::<Vec<_>>(),
        "period": def
            .columns
            .period()
            .map(|(from, to)| [from.to_string(), to.to_string()]),
        "enforcement": def.enforcement.map(|enforcement| match enforcement {
            Enforcement::Enforced => "enforced",
            Enforcement::NotEnforced => "not_enforced",
        }),
        "deferrability": def.deferrability.map(|deferrability| match deferrability {
            Deferrability::NotDeferrable => "not_deferrable",
            Deferrability::DeferrableInitiallyImmediate => "deferrable_initially_immediate",
            Deferrability::DeferrableInitiallyDeferred => "deferrable_initially_deferred",
        }),
    })
}

/// A relation's table as `{"schema", "table"}`; a relation joins entities,
/// whose FROM item is always a named table.
fn table(item: &FromItem) -> Json {
    match item {
        FromItem::Table(table) => json!({
            "schema": table.name.schema().map(|schema| schema.to_string()),
            "table": table.name.table().to_string(),
        }),
        other => json!({"schema": null, "table": other.qualifier().to_string()}),
    }
}
