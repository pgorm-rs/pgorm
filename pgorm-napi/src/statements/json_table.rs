//! `JSON_TABLE`, a FROM item reading rows out of a JSON document, over
//! pgorm-query's builder. Its paths are strings pgorm-query writes as escaped
//! literals, because PostgreSQL refuses a parameter there; its PASSING values
//! are bound.

use neon::prelude::*;
use pgorm::pgorm_query::{Func, JsonTableBehavior, JsonTableColumn};

use super::{
    Node,
    args::{absent, arg, choice, items, name_at, node, optional_name, refuse},
    data_type,
    json::{input, path, query_behavior, shaping, value_behavior, variables},
};

pub(super) const EXPORTS: &[(&str, super::Build)] = &[
    ("jsonTable", json_table),
    ("jsonTableOrdinality", ordinality),
    ("jsonTableValue", value_column),
    ("jsonTableQuery", query_column),
    ("jsonTableExists", exists_column),
    ("jsonTableNested", nested),
];

/// The columns of the array at `index`: at least one, each a
/// `JsonTableColumn`, as PostgreSQL refuses an empty list.
fn columns(
    cx: &mut FunctionContext,
    index: usize,
) -> NeonResult<(JsonTableColumn, Vec<JsonTableColumn>)> {
    let value = arg(cx, index);
    let mut columns = Vec::new();
    for column in items(cx, value)? {
        match node(cx, column) {
            Some(Node::JsonTableColumn(column)) => columns.push(column),
            _ => return refuse(cx, "JSON_TABLE's columns are JsonTableColumn's"),
        }
    }
    if columns.is_empty() {
        return refuse(cx, "JSON_TABLE takes at least one column");
    }
    let first = columns.remove(0);
    Ok((first, columns))
}

/// The optional column path at `index`.
fn column_path(cx: &mut FunctionContext, index: usize) -> NeonResult<Option<String>> {
    let value = arg(cx, index);
    if absent(cx, value) {
        Ok(None)
    } else {
        path(cx, index).map(Some)
    }
}

/// `jsonTableOrdinality(name)`: `name FOR ORDINALITY`.
// [spec:pgorm:req:napi.sql-json]
fn ordinality(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = name_at(cx, 0)?;
    Ok(Node::JsonTableColumn(JsonTableColumn::ordinality(name)))
}

/// `jsonTableValue(name, type, path, onEmpty, onError)`: the scalar its path
/// finds, read as `JSON_VALUE` reads it.
fn value_column(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = name_at(cx, 0)?;
    let kind = arg(cx, 1);
    let kind = data_type::column_type(cx, kind)?;
    let mut column = JsonTableColumn::value(name, kind);
    if let Some(path) = column_path(cx, 2)? {
        column = column.path(path);
    }
    if let Some(behavior) = value_behavior(cx, 3)? {
        column = column.on_empty(behavior);
    }
    if let Some(behavior) = value_behavior(cx, 4)? {
        column = column.on_error(behavior);
    }
    Ok(Node::JsonTableColumn(column.into()))
}

/// `jsonTableQuery(name, type, path, shaping, onEmpty, onError)`: the JSON
/// its path finds, read as `JSON_QUERY` reads it.
fn query_column(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = name_at(cx, 0)?;
    let kind = arg(cx, 1);
    let kind = data_type::column_type(cx, kind)?;
    let mut column = JsonTableColumn::query(name, kind);
    if let Some(path) = column_path(cx, 2)? {
        column = column.path(path);
    }
    column = match shaping(cx, 3)? {
        None => column,
        Some("withWrapper") => column.with_wrapper(),
        Some("withConditionalWrapper") => column.with_conditional_wrapper(),
        Some(_) => column.omit_quotes(),
    };
    if let Some(behavior) = query_behavior(cx, 4)? {
        column = column.on_empty(behavior);
    }
    if let Some(behavior) = query_behavior(cx, 5)? {
        column = column.on_error(behavior);
    }
    Ok(Node::JsonTableColumn(column.into()))
}

/// `jsonTableExists(name, type, path, onError)`: whether its path finds
/// anything. There is no `onEmpty`: finding nothing is false.
fn exists_column(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = name_at(cx, 0)?;
    let kind = arg(cx, 1);
    let kind = data_type::column_type(cx, kind)?;
    let mut column = JsonTableColumn::exists(name, kind);
    if let Some(path) = column_path(cx, 2)? {
        column = column.path(path);
    }
    let behavior = arg(cx, 3);
    if !absent(cx, behavior) {
        column = column.on_error(
            match choice(
                cx,
                3,
                "an EXISTS column's behaviour",
                &["true", "false", "unknown", "error"],
            )? {
                "true" => pgorm::pgorm_query::JsonExistsBehavior::True,
                "false" => pgorm::pgorm_query::JsonExistsBehavior::False,
                "unknown" => pgorm::pgorm_query::JsonExistsBehavior::Unknown,
                _ => pgorm::pgorm_query::JsonExistsBehavior::Error,
            },
        );
    }
    Ok(Node::JsonTableColumn(column.into()))
}

/// `jsonTableNested(path, columns, pathName)`: `NESTED PATH path COLUMNS
/// (..)`, a row per item its path finds under the parent row's, joined to it
/// as an outer join would be.
fn nested(cx: &mut FunctionContext) -> NeonResult<Node> {
    let path = path(cx, 0)?;
    let (first, rest) = columns(cx, 1)?;
    let mut nested = rest
        .into_iter()
        .fold(JsonTableColumn::nested(path, first), |nested, column| {
            nested.column(column)
        });
    let path_name = arg(cx, 2);
    if let Some(name) = optional_name(cx, path_name)? {
        nested = nested.path_name(name);
    }
    Ok(Node::JsonTableColumn(nested.into()))
}

/// `jsonTable(context, path, columns, alias, passing, pathName, onError)`:
/// `JSON_TABLE(..) AS "alias"`, a FROM item. The alias is required, as pgorm
/// names every FROM item that is not a table.
// [spec:pgorm:req:napi.sql-json]
fn json_table(cx: &mut FunctionContext) -> NeonResult<Node> {
    let context = arg(cx, 0);
    let context = input(cx, context)?;
    let root = path(cx, 1)?;
    let (first, rest) = columns(cx, 2)?;
    let alias = arg(cx, 3);
    if absent(cx, alias) {
        return refuse(
            cx,
            "JSON_TABLE needs its alias, as every FROM item that is not a table does",
        );
    }
    let alias = super::args::name(cx, alias)?;
    let mut table = rest
        .into_iter()
        .fold(Func::json_table(context, root, first), |table, column| {
            table.column(column)
        });
    for (value, name) in variables(cx, 4)? {
        table = table.passing(value, name);
    }
    let path_name = arg(cx, 5);
    if let Some(name) = optional_name(cx, path_name)? {
        table = table.path_name(name);
    }
    let behavior = arg(cx, 6);
    if !absent(cx, behavior) {
        table = table.on_error(
            match choice(cx, 6, "JSON_TABLE's behaviour", &["error", "empty"])? {
                "error" => JsonTableBehavior::Error,
                _ => JsonTableBehavior::Empty,
            },
        );
    }
    Ok(Node::FromItem(table.alias(alias)))
}
