//! `CREATE INDEX` and `DROP INDEX`. An index's entries are columns or
//! expressions, each with an operator class and an order; the index takes
//! uniqueness, `NULLS NOT DISTINCT`, an access method, `INCLUDE`d columns and
//! a partial-index predicate. `CONCURRENTLY` is not offered: PostgreSQL
//! refuses it inside a transaction, where pgorm runs DDL as often as not.

use neon::prelude::*;
use pgorm::pgorm_query::{Index, IndexColumn, IndexCreateStatement, IndexOrder, IndexType};

use super::{
    super::{
        Node,
        args::{arg, name, name_at, node, predicate_at, refuse},
    },
    Ddl, Part,
    options::{flag, get, name_option, names, object, pick, relation_at},
};

pub(super) const EXPORTS: &[(&str, super::super::Build)] = &[
    ("ddlCreateIndex", create_index),
    ("ddlIndexColumn", index_column),
    ("ddlIndexUnique", index_unique),
    ("ddlIndexNullsNotDistinct", index_nulls_not_distinct),
    ("ddlIndexIfNotExists", index_if_not_exists),
    ("ddlIndexUsing", index_using),
    ("ddlIndexInclude", index_include),
    ("ddlIndexWhere", index_where),
    ("ddlDropIndex", drop_index),
];

/// An index entry: a column's name, an expression, or `{ on, order,
/// operatorClass }` over either.
// [spec:pgorm:req:napi.schema-indexes]
fn entry<'cx>(cx: &mut Cx<'cx>, value: Handle<'cx, JsValue>) -> NeonResult<IndexColumn> {
    if value.is_a::<JsString, _>(cx) {
        return Ok(IndexColumn::name(name(cx, value)?));
    }
    match node(cx, value) {
        Some(Node::Expr(expr)) => return Ok(IndexColumn::expr(expr)),
        Some(other) => {
            let what = other.describe();
            return refuse(
                cx,
                format!("an index entry is a column's name or an expression, not {what}"),
            );
        }
        None => {}
    }
    let Ok(spec) = value.downcast::<JsObject, _>(cx) else {
        return cx.throw_type_error("an index entry is a name, an expression or { on, .. }");
    };
    for key in spec.get_own_property_names(cx)?.to_vec(cx)? {
        let key = crate::values::read::string(cx, key)?;
        if !["on", "order", "operatorClass"].contains(&key.as_str()) {
            return cx.throw_type_error(format!(
                "{key:?} is no part of an index entry; its parts are on, order, operatorClass"
            ));
        }
    }
    let on: Handle<JsValue> = spec.get_value(cx, "on")?;
    if on.is_a::<JsObject, _>(cx) && node(cx, on).is_none() {
        return cx.throw_type_error("an index entry's on is a column's name or an expression");
    }
    let mut column = entry(cx, on)?;
    let spec = Some(spec);
    if let Some(class) = name_option(cx, spec, "operatorClass")? {
        column = column.operator_class(class);
    }
    if let Some(order) = get(cx, spec, "order")? {
        let order = pick(
            cx,
            order,
            "an index entry's order",
            &[("asc", IndexOrder::Asc), ("desc", IndexOrder::Desc)],
        )?;
        column = column.order(order);
    }
    Ok(column)
}

fn receiver(cx: &mut FunctionContext) -> NeonResult<IndexCreateStatement> {
    super::receiver(cx, "a CREATE INDEX", |part| match part {
        Part::Statement(Ddl::CreateIndex(index)) => Ok(index),
        other => Err(other.describe()),
    })
}

/// `ddlCreateIndex(table, entry, { name })`: an index over its first entry,
/// named when the options say and named by PostgreSQL otherwise.
// [spec:pgorm:req:napi.schema-indexes]
fn create_index(cx: &mut FunctionContext) -> NeonResult<Node> {
    let table = relation_at(cx, 0)?;
    let first = arg(cx, 1);
    let first = entry(cx, first)?;
    let options = object(cx, 2, &["name"])?;
    let mut created = Index::create(table, first);
    if let Some(name) = name_option(cx, options, "name")? {
        created.name(name);
    }
    Ok(Ddl::CreateIndex(created).into())
}

fn index_column(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut created = receiver(cx)?;
    let column = arg(cx, 1);
    let column = entry(cx, column)?;
    created.col(column);
    Ok(Ddl::CreateIndex(created).into())
}

fn index_unique(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut created = receiver(cx)?;
    created.unique();
    Ok(Ddl::CreateIndex(created).into())
}

/// `NULLS NOT DISTINCT`, which PostgreSQL defines for a unique index alone,
/// and so makes the index unique.
// [spec:pgorm:req:napi.schema-indexes]
fn index_nulls_not_distinct(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut created = receiver(cx)?;
    created.unique().nulls_not_distinct();
    Ok(Ddl::CreateIndex(created).into())
}

fn index_if_not_exists(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut created = receiver(cx)?;
    created.if_not_exists();
    Ok(Ddl::CreateIndex(created).into())
}

/// `ddlIndexUsing(index, method)`: the access method, any the server has —
/// `btree`, `hash` and `gin` by pgorm-query's own variants, another by name.
// [spec:pgorm:req:napi.schema-indexes]
fn index_using(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut created = receiver(cx)?;
    let value = arg(cx, 1);
    let method = name(cx, value)?;
    let spelled = crate::values::read::string(cx, value)?;
    let method = match spelled.as_str() {
        "btree" => IndexType::BTree,
        "hash" => IndexType::Hash,
        "gin" => IndexType::Gin,
        _ => IndexType::Named(method),
    };
    created.index_type(method);
    Ok(Ddl::CreateIndex(created).into())
}

fn index_include(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut created = receiver(cx)?;
    let columns = arg(cx, 1);
    let columns = names(cx, columns, "an index's include list")?;
    created.include(columns);
    Ok(Ddl::CreateIndex(created).into())
}

/// `ddlIndexWhere(index, predicate)`: the partial index's predicate, ANDed to
/// one already there.
fn index_where(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut created = receiver(cx)?;
    let predicate = predicate_at(cx, 1)?;
    created.cond_where(predicate);
    Ok(Ddl::CreateIndex(created).into())
}

/// `ddlDropIndex(table, name, { ifExists })`: the index in its table's
/// schema.
// [spec:pgorm:req:napi.schema-indexes]
fn drop_index(cx: &mut FunctionContext) -> NeonResult<Node> {
    let table = relation_at(cx, 0)?;
    let name = name_at(cx, 1)?;
    let options = object(cx, 2, &["ifExists"])?;
    let mut drop = Index::drop(name);
    drop.table(table);
    if flag(cx, options, "ifExists")? {
        drop.if_exists();
    }
    Ok(Ddl::DropIndex(drop).into())
}
