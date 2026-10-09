//! INSERT, UPDATE and DELETE over pgorm-query's statements, with the state
//! the binding tracks beside each to refuse one that cannot run: an INSERT's
//! columns before its rows, an UPDATE's assignments, and the predicate or
//! explicit `allRows()` an UPDATE or DELETE needs.

use std::collections::HashSet;

use neon::prelude::*;
use pgorm::pgorm_query::{
    DeleteStatement, InsertStatement, Name, Overriding, Query, UpdateStatement, Values,
};

use super::{
    Node,
    args::{
        arg, choice, list, name, name_at, operand, operands_at, predicate_at, refuse, select_at,
        this,
    },
    returning, table, with,
};

pub(super) const EXPORTS: &[(&str, super::Build)] = &[
    ("insertNew", insert_new),
    ("insertColumns", insert_columns),
    ("insertValues", insert_values),
    ("insertSelect", insert_select),
    ("insertDefaults", insert_defaults),
    ("insertOverriding", insert_overriding),
    ("insertOnConflict", insert_on_conflict),
    ("updateNew", update_new),
    ("updateSet", update_set),
    ("deleteNew", delete_new),
    ("writeWhere", write_where),
    ("writeAllRows", write_all_rows),
    ("writeFrom", write_from),
    ("writeReturning", write_returning),
    ("writeWith", write_with),
];

/// Where an INSERT's rows come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rows {
    None,
    Values,
    Select,
    Defaults,
}

#[derive(Debug, Clone)]
pub(crate) struct Insert {
    pub(crate) inner: InsertStatement,
    columns: Vec<String>,
    rows: Rows,
}

#[derive(Debug, Clone)]
pub(crate) struct Update {
    pub(crate) inner: UpdateStatement,
    guarded: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Delete {
    pub(crate) inner: DeleteStatement,
    guarded: bool,
}

/// The SQL and values a write builds, or why it cannot run.
// [spec:pgorm:req:napi.writes]
pub(crate) fn built(node: &Node) -> Option<Result<(String, Values), &'static str>> {
    Some(match node {
        Node::Insert(insert) if insert.rows == Rows::None => {
            Err("an INSERT needs rows: values(..), select(..) or defaultValues()")
        }
        Node::Insert(insert) => Ok(insert.inner.build()),
        Node::Update(update) if update.inner.get_values().is_empty() => {
            Err("an UPDATE needs at least one set(column, value)")
        }
        Node::Update(update) if !update.guarded => {
            Err("an UPDATE needs where(..), or allRows() to say it means every row")
        }
        Node::Update(update) => Ok(update.inner.build()),
        Node::Delete(delete) if !delete.guarded => {
            Err("a DELETE needs where(..), or allRows() to say it means every row")
        }
        Node::Delete(delete) => Ok(delete.inner.build()),
        _ => return None,
    })
}

fn insert_receiver<'cx>(cx: &mut FunctionContext<'cx>) -> NeonResult<Insert> {
    match this(cx, 0)? {
        Node::Insert(insert) => Ok(insert),
        other => refuse(cx, format!("expected an INSERT, got {}", other.describe())),
    }
}

fn target<'cx>(cx: &mut FunctionContext<'cx>) -> NeonResult<pgorm::pgorm_query::NamedTable> {
    match this(cx, 0)? {
        Node::Table(table) => Ok(table),
        other => refuse(
            cx,
            format!("a write's target is a Table, not {}", other.describe()),
        ),
    }
}

/// `insertNew(table)`.
// [spec:pgorm:req:napi.writes]
fn insert_new(cx: &mut FunctionContext) -> NeonResult<Node> {
    let table = target(cx)?;
    Ok(Node::Insert(Insert {
        inner: Query::insert().into_table(table).to_owned(),
        columns: Vec::new(),
        rows: Rows::None,
    }))
}

/// `insertColumns(insert, columns)`: the distinct columns the rows fill,
/// named before any row.
fn insert_columns(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut insert = insert_receiver(cx)?;
    if insert.rows != Rows::None || !insert.columns.is_empty() {
        return refuse(cx, "an INSERT names its columns once, before any row");
    }
    let mut seen = HashSet::new();
    let mut names: Vec<Name> = Vec::new();
    for column in list(cx, 1)? {
        let column = name(cx, column)?;
        if !seen.insert(column.to_string()) {
            return refuse(cx, format!("{:?} is named twice", column.to_string()));
        }
        names.push(column);
    }
    if names.is_empty() {
        return refuse(cx, "an INSERT without columns writes defaultValues()");
    }
    insert.columns = names.iter().map(|name| name.to_string()).collect();
    insert.inner.columns(names);
    Ok(Node::Insert(insert))
}

/// `insertValues(insert, row)`: one row, as many operands as columns.
fn insert_values(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut insert = insert_receiver(cx)?;
    if insert.columns.is_empty() || matches!(insert.rows, Rows::Select | Rows::Defaults) {
        return refuse(
            cx,
            "values(..) follows columns(..), and mixes with neither select(..) nor defaultValues()",
        );
    }
    let row = operands_at(cx, 1)?;
    if let Err(error) = insert.inner.values(row) {
        return refuse(cx, error.to_string());
    }
    insert.rows = Rows::Values;
    Ok(Node::Insert(insert))
}

/// `insertSelect(insert, select)`: the rows a query yields, as many columns
/// as the INSERT names.
fn insert_select(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut insert = insert_receiver(cx)?;
    if insert.columns.is_empty() || insert.rows != Rows::None {
        return refuse(
            cx,
            "select(..) follows columns(..), and is an INSERT's only rows",
        );
    }
    let select = select_at(cx, 1)?;
    if let Err(error) = insert.inner.select_from(select) {
        return refuse(cx, error.to_string());
    }
    insert.rows = Rows::Select;
    Ok(Node::Insert(insert))
}

/// `insertDefaults(insert)`: one row of defaults.
fn insert_defaults(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut insert = insert_receiver(cx)?;
    if !insert.columns.is_empty() || insert.rows != Rows::None {
        return refuse(cx, "defaultValues() mixes with neither columns nor rows");
    }
    insert.inner.or_default_values();
    insert.rows = Rows::Defaults;
    Ok(Node::Insert(insert))
}

/// `insertOverriding(insert, which)`: `OVERRIDING SYSTEM VALUE` or `USER
/// VALUE`, for identity columns.
fn insert_overriding(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut insert = insert_receiver(cx)?;
    let overriding = match choice(cx, 1, "overriding", &["systemValue", "userValue"])? {
        "systemValue" => Overriding::SystemValue,
        _ => Overriding::UserValue,
    };
    insert.inner.overriding(overriding);
    Ok(Node::Insert(insert))
}

/// `insertOnConflict(insert, conflict)`: a completed conflict action.
fn insert_on_conflict(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut insert = insert_receiver(cx)?;
    let conflict = arg(cx, 1);
    let conflict = super::conflict::clause(cx, conflict)?;
    insert.inner.on_conflict(conflict);
    Ok(Node::Insert(insert))
}

/// `updateNew(table)`.
// [spec:pgorm:req:napi.writes]
fn update_new(cx: &mut FunctionContext) -> NeonResult<Node> {
    let table = target(cx)?;
    Ok(Node::Update(Update {
        inner: Query::update().table(table).to_owned(),
        guarded: false,
    }))
}

/// `updateSet(update, column, value)`: one assignment, each column once.
fn update_set(cx: &mut FunctionContext) -> NeonResult<Node> {
    let Node::Update(mut update) = this(cx, 0)? else {
        return refuse(cx, "expected an UPDATE");
    };
    let column = name_at(cx, 1)?;
    let value = arg(cx, 2);
    let value = operand(cx, value)?;
    let text = column.to_string();
    if update
        .inner
        .get_values()
        .iter()
        .any(|(assigned, _)| assigned.to_string() == text)
    {
        return refuse(cx, format!("{text:?} is assigned twice"));
    }
    update.inner.value(column, value);
    Ok(Node::Update(update))
}

/// `deleteNew(table)`.
// [spec:pgorm:req:napi.writes]
fn delete_new(cx: &mut FunctionContext) -> NeonResult<Node> {
    let table = target(cx)?;
    Ok(Node::Delete(Delete {
        inner: Query::delete().from_table(table).to_owned(),
        guarded: false,
    }))
}

/// Apply `$body` to the pgorm-query statement inside a write node, as
/// `$inner`, for the kinds of write listed; any other node is refused.
macro_rules! on_write {
    ($cx:expr, $node:expr, $inner:ident => $body:expr, $($kind:ident)|+) => {
        match &mut $node {
            $(Node::$kind(write) => {
                let $inner = write;
                $body;
            })+
            other => {
                let what = other.describe();
                return refuse($cx, format!("{what} takes no such clause"));
            }
        }
    };
}

/// `writeWhere(write, predicate)`: an UPDATE's or DELETE's predicate, ANDed to
/// any before it.
fn write_where(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut node = this(cx, 0)?;
    let predicate = predicate_at(cx, 1)?;
    on_write!(cx, node, write => {
        write.inner.cond_where(predicate);
        write.guarded = true;
    }, Update | Delete);
    Ok(node)
}

/// `writeAllRows(write)`: say an UPDATE or DELETE means every row it reaches.
/// It removes no predicate.
fn write_all_rows(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut node = this(cx, 0)?;
    on_write!(cx, node, write => write.guarded = true, Update | Delete);
    Ok(node)
}

/// `writeFrom(write, item)`: an UPDATE's FROM item or a DELETE's USING item.
fn write_from(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut node = this(cx, 0)?;
    let item = arg(cx, 1);
    let item = table::from_item(cx, item)?;
    match &mut node {
        Node::Update(update) => {
            update.inner.from(item);
        }
        Node::Delete(delete) => {
            delete.inner.using(item);
        }
        other => {
            let what = other.describe();
            return refuse(cx, format!("{what} reads no FROM or USING item"));
        }
    }
    Ok(node)
}

/// `writeReturning(write, items, oldAs, newAs)`: the RETURNING list.
// [spec:pgorm:req:napi.writes]
fn write_returning(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut node = this(cx, 0)?;
    let clause = returning::clause(cx, 1)?;
    on_write!(cx, node, write => write.inner.returning(clause), Insert | Update | Delete);
    Ok(node)
}

/// `writeWith(write, clause)`: the write's WITH clause, replacing any.
fn write_with(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut node = this(cx, 0)?;
    let clause = with::clause_at(cx, 1)?;
    on_write!(cx, node, write => write.inner.with(clause), Insert | Update | Delete);
    Ok(node)
}
