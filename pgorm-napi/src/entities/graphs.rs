//! Registered graphs as JavaScript reaches them: a shape found by name, a
//! query over it under the aliases JavaScript chooses, its rows decoded by
//! Rust's `GraphRow`, and a keyset cursor through `SelectGraph::cursor_by`.

use std::sync::Arc;

use neon::{prelude::*, types::Finalize};
use pgorm::pgorm_query::{Expr, Name};

use super::{
    convert::{GraphRows, column_value, entity_failure},
    graph::{Boundary, Change, CursorPlan, Factory, Query},
    info::ColumnInfo,
    inspected, strings, throw,
};
use crate::{
    connect::job::{Job, JobHandle, Output},
    errors::Failure,
    statements::Node,
    values::read,
};

#[derive(Debug)]
struct GraphHandle(Arc<dyn Factory>);

impl Finalize for GraphHandle {}

#[derive(Debug)]
struct GraphQueryHandle(Query);

impl Finalize for GraphQueryHandle {}

#[derive(Debug)]
struct GraphCursorHandle(Query, CursorPlan);

impl Finalize for GraphCursorHandle {}

pub(super) fn export(cx: &mut ModuleContext) -> NeonResult<()> {
    cx.export_function("graphNames", graph_names)?;
    cx.export_function("graphGet", graph_get)?;
    cx.export_function("graphDescribe", graph_describe)?;
    cx.export_function("graphFind", graph_find)?;
    cx.export_function("graphQueryAliases", query_aliases)?;
    cx.export_function("graphQueryChange", query_change)?;
    cx.export_function("graphQueryCol", query_col)?;
    cx.export_function("graphQueryInspect", query_inspect)?;
    cx.export_function("graphQueryJob", query_job)?;
    cx.export_function("graphCursorNew", cursor_new)?;
    cx.export_function("graphCursorBound", cursor_bound)?;
    cx.export_function("graphCursorWindow", cursor_window)?;
    cx.export_function("graphCursorDirection", cursor_direction)?;
    cx.export_function("graphCursorJob", cursor_job)?;
    Ok(())
}

/// The cursor whose handle is the receiver: its query and its plan.
fn cursor(cx: &mut FunctionContext) -> NeonResult<(Query, CursorPlan)> {
    let handle = cx.argument::<JsBox<GraphCursorHandle>>(0)?;
    Ok((handle.0.clone(), handle.1.clone()))
}

fn graph_names(mut cx: FunctionContext) -> JsResult<JsValue> {
    let registry = held!(cx, super::RegistryHandle, 0);
    strings(
        &mut cx,
        registry
            .graphs
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .into_iter(),
    )
}

/// `graphGet(registry, name)`: the graph shape registered as `name`.
// [spec:pgorm:req:napi.entity-graphs]
fn graph_get(mut cx: FunctionContext) -> JsResult<JsValue> {
    let registry = held!(cx, super::RegistryHandle, 0);
    let name = text!(cx, 1);
    match registry.graphs.get(&name) {
        Some(factory) => Ok(cx.boxed(GraphHandle(factory.clone())).upcast()),
        None => throw(
            &mut cx,
            Failure::Construction(format!("no graph is registered as {name:?} in this module")),
        ),
    }
}

fn graph_describe(mut cx: FunctionContext) -> JsResult<JsValue> {
    let graph = held!(cx, GraphHandle, 0);
    Ok(cx.string(graph.info().describe().to_string()).upcast())
}

/// The aliases a graph's slots are joined under: the ones given, one per
/// slot, each an identifier, none repeated nor the root's table; or else
/// `g1`, `g2`, .., a name the root's table takes skipped.
fn aliases(cx: &mut FunctionContext, factory: &dyn Factory) -> NeonResult<Vec<String>> {
    let sources = &factory.info().bindings.sources;
    let root = sources
        .first()
        .map(|source| source.entity.table.clone())
        .unwrap_or_default();
    let slots = sources.len().saturating_sub(1);
    let given = cx.argument::<JsValue>(1)?;
    if given.is_a::<JsNull, _>(cx) || given.is_a::<JsUndefined, _>(cx) {
        let mut names = Vec::with_capacity(slots);
        let mut next = 1;
        while names.len() < slots {
            let name = format!("g{next}");
            next += 1;
            if name != root {
                names.push(name);
            }
        }
        return Ok(names);
    }
    let list = given.downcast_or_throw::<JsArray, _>(cx)?.to_vec(cx)?;
    if list.len() != slots {
        return read::refuse(
            cx,
            format!(
                "the graph joins {slots} slots, and {} aliases were given",
                list.len()
            ),
        );
    }
    let mut names = Vec::with_capacity(slots);
    for item in list {
        let name = read::string(cx, item)?;
        if name.is_empty() || name.len() > 63 || name.contains('\0') {
            return read::refuse(cx, "an alias is 1–63 UTF-8 bytes without NUL");
        }
        if name == root || names.contains(&name) {
            return read::refuse(cx, format!("the alias {name:?} names a source twice"));
        }
        names.push(name);
    }
    Ok(names)
}

/// `graphFind(graph, aliases)`: the graph built by its factory under the
/// aliases.
// [spec:pgorm:req:napi.entity-graphs]
fn graph_find(mut cx: FunctionContext) -> JsResult<JsValue> {
    let factory = held!(cx, GraphHandle, 0);
    let aliases = aliases(&mut cx, factory.as_ref())?;
    Ok(cx.boxed(GraphQueryHandle(factory.find(aliases))).upcast())
}

fn query_aliases(mut cx: FunctionContext) -> JsResult<JsValue> {
    let query = held!(cx, GraphQueryHandle, 0);
    strings(&mut cx, query.aliases().to_vec().into_iter())
}

fn query_change(mut cx: FunctionContext) -> JsResult<JsValue> {
    let query = held!(cx, GraphQueryHandle, 0);
    let change = match super::step(&mut cx)? {
        super::Change::Filter(condition) => Change::Filter(condition),
        super::Change::Order(expr, order, nulls) => Change::Order(expr, order, nulls),
        super::Change::Limit(_) | super::Change::Offset(_) => {
            return cx.throw_type_error("a registered graph takes where and orderBy steps");
        }
    };
    Ok(cx.boxed(GraphQueryHandle(query.change(change))).upcast())
}

/// `graphQueryCol(query, source, column)`: a column of a decoded source,
/// qualified as the query names it — the root by its table, a slot by its
/// alias.
// [spec:pgorm:req:napi.entity-graphs]
fn query_col(mut cx: FunctionContext) -> JsResult<JsValue> {
    let query = held!(cx, GraphQueryHandle, 0);
    let source = cx.argument::<JsNumber>(1)?.value(&mut cx);
    let column = text!(cx, 2);
    let sources = &query.info().bindings.sources;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = source as usize;
    let Some(found) = (source.fract() == 0.0 && source >= 0.0)
        .then(|| sources.get(index))
        .flatten()
    else {
        return read::refuse(&mut cx, format!("the graph has no source {source}"));
    };
    if let Err(failure) = found.entity.column(&column) {
        return throw(&mut cx, failure);
    }
    let qualifier = match index {
        0 => found.entity.table.clone(),
        _ => query.aliases()[index - 1].clone(),
    };
    let expr = Expr::col((Name::runtime(qualifier), Name::runtime(column)));
    Ok(cx.boxed(Node::Expr(expr.into())).upcast())
}

fn query_inspect(mut cx: FunctionContext) -> JsResult<JsValue> {
    let query = held!(cx, GraphQueryHandle, 0);
    let optional = cx.argument::<JsBoolean>(1)?.value(&mut cx);
    let compiled = query.compile(optional);
    inspected(&mut cx, compiled)
}

fn sources(query: &Query) -> Vec<Arc<super::info::EntityInfo>> {
    query
        .info()
        .bindings
        .sources
        .iter()
        .map(|source| source.entity.clone())
        .collect()
}

/// `graphQueryJob(query, optional)`: `all`, or `one_opt` with its `LIMIT 1`.
fn query_job(mut cx: FunctionContext) -> JsResult<JsValue> {
    let query = held!(cx, GraphQueryHandle, 0);
    let optional = cx.argument::<JsBoolean>(1)?.value(&mut cx);
    let job: Job = Box::new(move |db, secrets| {
        Box::pin(async move {
            let rows = query
                .run(db, optional)
                .await
                .map_err(|error| entity_failure(&error, secrets))?;
            Ok(Box::new(GraphRows(sources(&query), rows)) as Output)
        })
    });
    Ok(cx.boxed(JobHandle::new(job)).upcast())
}

/// `graphCursorNew(query, column)`: a cursor ordered by one root column.
// [spec:pgorm:req:napi.entity-graphs]
fn cursor_new(mut cx: FunctionContext) -> JsResult<JsValue> {
    let query = held!(cx, GraphQueryHandle, 0);
    let column = text!(cx, 1);
    let root = &query.info().bindings.sources[0].entity;
    if let Err(failure) = root.column(&column) {
        return throw(&mut cx, failure);
    }
    let plan = CursorPlan {
        column,
        ..CursorPlan::default()
    };
    Ok(cx.boxed(GraphCursorHandle(query, plan)).upcast())
}

/// The whole key a full boundary names: the order column, the root's other
/// key columns, then each slot's key, as `cursor_by` installs them.
fn keyset(query: &Query, column: &str) -> Result<Vec<ColumnInfo>, Failure> {
    let sources = &query.info().bindings.sources;
    let mut columns = vec![sources[0].entity.column(column)?.clone()];
    for (index, source) in sources.iter().enumerate() {
        for key in &source.entity.primary_key {
            if index != 0 || key != column {
                columns.push(source.entity.column(key)?.clone());
            }
        }
    }
    Ok(columns)
}

/// `graphCursorBound(cursor, before, values, full)`: the cursor with a
/// boundary — the order column's value, or with `full` the whole key's —
/// each value converted to its column's declared kind.
// [spec:pgorm:req:napi.entity-graphs]
fn cursor_bound(mut cx: FunctionContext) -> JsResult<JsValue> {
    let (query, mut plan) = cursor(&mut cx)?;
    let before = cx.argument::<JsBoolean>(1)?.value(&mut cx);
    let values = cx.argument::<JsArray>(2)?.to_vec(&mut cx)?;
    let full = cx.argument::<JsBoolean>(3)?.value(&mut cx);
    let columns = match keyset(&query, &plan.column) {
        Ok(columns) if full => columns,
        Ok(columns) => columns.into_iter().take(1).collect(),
        Err(failure) => return throw(&mut cx, failure),
    };
    if values.len() != columns.len() {
        return read::refuse(
            &mut cx,
            format!(
                "the boundary takes {} values, and {} were given",
                columns.len(),
                values.len()
            ),
        );
    }
    let mut bound = Vec::with_capacity(values.len());
    for (column, value) in columns.iter().zip(values) {
        bound.push(column_value(&mut cx, column, value)?);
    }
    let boundary = Some(Boundary {
        values: bound,
        full,
    });
    if before {
        plan.before = boundary;
    } else {
        plan.after = boundary;
    }
    Ok(cx.boxed(GraphCursorHandle(query, plan)).upcast())
}

fn cursor_window(mut cx: FunctionContext) -> JsResult<JsValue> {
    let (query, mut plan) = cursor(&mut cx)?;
    let last = cx.argument::<JsBoolean>(1)?.value(&mut cx);
    let rows = cx.argument::<JsNumber>(2)?.value(&mut cx);
    if rows.fract() != 0.0 || !(0.0..=9_007_199_254_740_991.0).contains(&rows) {
        return read::refuse(&mut cx, "a window's rows are a non-negative integer");
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rows = rows as u64;
    plan.window = Some((last, rows));
    Ok(cx.boxed(GraphCursorHandle(query, plan)).upcast())
}

fn cursor_direction(mut cx: FunctionContext) -> JsResult<JsValue> {
    let (query, mut plan) = cursor(&mut cx)?;
    plan.descending = cx.argument::<JsBoolean>(1)?.value(&mut cx);
    Ok(cx.boxed(GraphCursorHandle(query, plan)).upcast())
}

/// `graphCursorJob(cursor)`: `Cursor::all`, a last window's rows returned
/// in the cursor's order.
fn cursor_job(mut cx: FunctionContext) -> JsResult<JsValue> {
    let (query, plan) = cursor(&mut cx)?;
    let job: Job = Box::new(move |db, secrets| {
        Box::pin(async move {
            let rows = query
                .cursor(db, plan)
                .await
                .map_err(|error| entity_failure(&error, secrets))?;
            Ok(Box::new(GraphRows(sources(&query), rows)) as Output)
        })
    });
    Ok(cx.boxed(JobHandle::new(job)).upcast())
}
