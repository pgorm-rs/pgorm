use serde_json::{Map, Value, json};

pub(crate) fn operations() -> Map<String, Value> {
    [
        (
            "json_exists",
            "pgorm_query::Func::json_exists, JsonExists::{passing, on_error}",
        ),
        (
            "json_value",
            "pgorm_query::Func::json_value, JsonValue::{passing, returning, on_empty, on_error}, JsonValueType::try_from",
        ),
        (
            "json_query",
            "pgorm_query::Func::json_query, JsonQuery::{passing, returning, with_wrapper, with_conditional_wrapper, omit_quotes, on_empty, on_error}",
        ),
        (
            "json_table",
            "pgorm_query::Func::json_table, JsonTable::{column, passing, path_name, on_error, alias}, JsonTableBehavior",
        ),
        (
            "json_table_column",
            "pgorm_query::JsonTableColumn::{ordinality, value, query, exists, nested}, JsonValueColumn::{path, on_empty, on_error}, JsonQueryColumn::{path, with_wrapper, with_conditional_wrapper, omit_quotes, on_empty, on_error}, JsonExistsColumn::{path, on_error}, JsonNestedColumns::{column, path_name}",
        ),
        (
            "json_behavior",
            "pgorm_query::{JsonExistsBehavior, JsonValueBehavior, JsonQueryBehavior}",
        ),
        (
            "json_object",
            "pgorm_query::Func::json_object, JsonObject::{entry, absent_on_null, with_unique_keys, returning}",
        ),
        (
            "json_array",
            "pgorm_query::Func::json_array, JsonArray::{element, null_on_null, returning}",
        ),
        (
            "json_array_query",
            "pgorm_query::Func::json_array_query, JsonArrayQuery::returning",
        ),
        (
            "json_objectagg",
            "pgorm_query::Func::json_objectagg, JsonObjectAgg::{absent_on_null, with_unique_keys, returning, filter}",
        ),
        (
            "json_arrayagg",
            "pgorm_query::Func::json_arrayagg, JsonArrayAgg::{order_by, null_on_null, returning, filter}",
        ),
        (
            "json_parse",
            "pgorm_query::Func::json, JsonParse::with_unique_keys",
        ),
        ("json_scalar", "pgorm_query::Func::json_scalar"),
        (
            "json_serialize",
            "pgorm_query::Func::json_serialize, JsonSerialize::returning",
        ),
        ("format_json", "pgorm_query::Expr::format_json"),
        (
            "is_json",
            "pgorm_query::Expr::is_json, JsonKind::with_unique_keys",
        ),
        (
            "is_not_json",
            "pgorm_query::Expr::is_not_json, JsonKind::with_unique_keys",
        ),
    ]
    .into_iter()
    .map(|(name, rust_api)| (name.into(), json!({"rust_api": rust_api, "features": []})))
    .collect()
}
