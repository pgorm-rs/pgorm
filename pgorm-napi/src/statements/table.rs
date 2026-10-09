//! Tables and the other FROM items: a named table with its schema and alias,
//! and an aliased subquery. A FROM item that is not a named table always has
//! an alias, as PostgreSQL requires, so its columns are qualified by a name
//! the caller chose.

use neon::prelude::*;
use pgorm::pgorm_query::{ColumnRef, Expr, FromItem, NamedTable, TableName};

use super::{
    Node,
    args::{arg, name_at, optional_name, refuse, select_at, this},
};

pub(super) const EXPORTS: &[(&str, super::Build)] = &[
    ("tableNew", table_new),
    ("tableAlias", table_alias),
    ("tableCol", table_col),
    ("tableStar", table_star),
    ("fromSubquery", from_subquery),
];

/// `tableNew(name, schema, alias)`.
// [spec:pgorm:req:napi.select]
fn table_new(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = name_at(cx, 0)?;
    let schema = arg(cx, 1);
    let name = match optional_name(cx, schema)? {
        Some(schema) => TableName::SchemaTable(schema, name),
        None => TableName::Table(name),
    };
    let alias = arg(cx, 2);
    let mut table = NamedTable::from(name);
    if let Some(alias) = optional_name(cx, alias)? {
        table = table.alias(alias);
    }
    Ok(Node::Table(table))
}

fn table_alias(cx: &mut FunctionContext) -> NeonResult<Node> {
    let Node::Table(table) = this(cx, 0)? else {
        return refuse(cx, "expected a Table");
    };
    let alias = name_at(cx, 1)?;
    Ok(Node::Table(table.alias(alias)))
}

/// The column reference a `Table` or FROM item qualifies `column` with: a
/// table's alias if it has one, else its own, schema-qualified, name; another
/// item's alias.
fn qualified(item: &Node, column: Option<pgorm::pgorm_query::Name>) -> Option<ColumnRef> {
    Some(match (item, column) {
        (Node::Table(table), Some(column)) => match (&table.alias, &table.name) {
            (Some(alias), _) => ColumnRef::TableColumn(alias.clone(), column),
            (None, TableName::Table(name)) => ColumnRef::TableColumn(name.clone(), column),
            (None, TableName::SchemaTable(schema, name)) => {
                ColumnRef::SchemaTableColumn(schema.clone(), name.clone(), column)
            }
        },
        (Node::Table(table), None) => ColumnRef::TableAsterisk(table.qualifier().clone()),
        (Node::FromItem(item), Some(column)) => {
            ColumnRef::TableColumn(item.qualifier().clone(), column)
        }
        (Node::FromItem(item), None) => ColumnRef::TableAsterisk(item.qualifier().clone()),
        _ => return None,
    })
}

/// `tableCol(item, name)`: one of a table's or FROM item's columns.
fn table_col(cx: &mut FunctionContext) -> NeonResult<Node> {
    let item = this(cx, 0)?;
    let column = name_at(cx, 1)?;
    match qualified(&item, Some(column)) {
        Some(column) => Ok(Node::Expr(Expr::col(column).into())),
        None => refuse(cx, "expected a Table or a FROM item"),
    }
}

/// `tableStar(item)`: every column of a table or FROM item, `"t".*`.
fn table_star(cx: &mut FunctionContext) -> NeonResult<Node> {
    let item = this(cx, 0)?;
    match qualified(&item, None) {
        Some(column) => Ok(Node::Expr(Expr::col(column).into())),
        None => refuse(cx, "expected a Table or a FROM item"),
    }
}

/// `fromSubquery(select, alias)`: `(SELECT ..) AS "alias"`.
// [spec:pgorm:req:napi.select]
fn from_subquery(cx: &mut FunctionContext) -> NeonResult<Node> {
    let select = select_at(cx, 0)?;
    let alias = name_at(cx, 1)?;
    Ok(Node::FromItem(FromItem::SubQuery(select, alias)))
}

/// The FROM item a `Table` or FROM-item argument stands for.
pub(super) fn from_item<'cx>(
    cx: &mut Cx<'cx>,
    value: Handle<'cx, JsValue>,
) -> NeonResult<FromItem> {
    match super::args::node(cx, value) {
        Some(Node::Table(table)) => Ok(FromItem::Table(table)),
        Some(Node::FromItem(item)) => Ok(item),
        Some(other) => {
            let what = other.describe();
            refuse(
                cx,
                format!("a FROM item is a Table or a FromItem, not {what}"),
            )
        }
        None => refuse(cx, "a FROM item is a Table or a FromItem"),
    }
}

/// The name a locking clause's `OF` gives an item: its qualifier, the alias
/// it answers to in the statement, which PostgreSQL requires unqualified.
pub(super) fn lock_target<'cx>(
    cx: &mut Cx<'cx>,
    value: Handle<'cx, JsValue>,
) -> NeonResult<NamedTable> {
    let item = from_item(cx, value)?;
    let qualifier = match &item {
        FromItem::Table(table) => table.qualifier().clone(),
        other => other.qualifier().clone(),
    };
    Ok(NamedTable::from(TableName::Table(qualifier)))
}
