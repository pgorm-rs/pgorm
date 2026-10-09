//! Common table expressions: a WITH clause of one or more, and the
//! recursive form, which holds exactly one with its optional SEARCH and
//! CYCLE clauses.

use neon::prelude::*;
use pgorm::pgorm_query::{
    AnyWithClause, CommonTableExpression, Cycle, RecursiveWithClause, Search, SearchOrder,
    WithClause,
};

use super::{
    Node,
    args::{absent, arg, choice, name, name_at, node, operand, refuse, this},
};

pub(super) const EXPORTS: &[(&str, super::Build)] = &[
    ("withNew", with_new),
    ("withCte", with_cte),
    ("withRecursive", with_recursive),
];

/// The common table expression at `index`: its name, its body — whose rows
/// are a `Select`'s, or a write's RETURNING rows — its column names and its
/// materialization.
// [spec:pgorm:req:napi.select]
fn expression<'cx>(
    cx: &mut FunctionContext<'cx>,
    index: usize,
) -> NeonResult<CommonTableExpression> {
    let table = name_at(cx, index)?;
    let body = arg(cx, index + 1);
    let mut expression = match node(cx, body) {
        Some(Node::Select(select)) => CommonTableExpression::new(table, select),
        Some(Node::Insert(insert)) => CommonTableExpression::new(table, insert.inner),
        Some(Node::Update(update)) => CommonTableExpression::new(table, update.inner),
        Some(Node::Delete(delete)) => CommonTableExpression::new(table, delete.inner),
        Some(other) => {
            let what = other.describe();
            return refuse(
                cx,
                format!("a common table expression's body is a statement, not {what}"),
            );
        }
        None => return refuse(cx, "a common table expression's body is a statement"),
    };
    let columns = arg(cx, index + 2);
    if !absent(cx, columns) {
        for column in super::args::items(cx, columns)? {
            expression.column(name(cx, column)?);
        }
    }
    let materialized = arg(cx, index + 3);
    if let Ok(flag) = materialized.downcast::<JsBoolean, _>(cx) {
        expression.materialized(flag.value(cx));
    } else if !absent(cx, materialized) {
        return refuse(cx, "materialized is a boolean");
    }
    Ok(expression)
}

/// `withNew(name, body, columns, materialized)`: `WITH name AS (body)`.
fn with_new(cx: &mut FunctionContext) -> NeonResult<Node> {
    let expression = expression(cx, 0)?;
    Ok(Node::With(WithClause::new(expression).into()))
}

/// `withCte(clause, name, body, columns, materialized)`: one more common
/// table expression, after the clause's others; a recursive clause holds
/// exactly one.
fn with_cte(cx: &mut FunctionContext) -> NeonResult<Node> {
    let Node::With(AnyWithClause::Plain(mut clause)) = this(cx, 0)? else {
        return refuse(
            cx,
            "a recursive WITH holds exactly one common table expression",
        );
    };
    let expression = expression(cx, 1)?;
    clause.cte(expression);
    Ok(Node::With(clause.into()))
}

/// `withRecursive(name, body, columns, materialized, search, cycle)`:
/// `WITH RECURSIVE`, with `SEARCH BREADTH | DEPTH FIRST BY expr SET name`
/// and `CYCLE expr SET name USING name` when they are given.
// [spec:pgorm:req:napi.select]
fn with_recursive(cx: &mut FunctionContext) -> NeonResult<Node> {
    let expression = expression(cx, 0)?;
    let mut clause = RecursiveWithClause::new(expression);
    let search = arg(cx, 4);
    if !absent(cx, search) {
        let order = match choice(cx, 4, "a search order", &["breadth", "depth"])? {
            "breadth" => SearchOrder::BREADTH,
            _ => SearchOrder::DEPTH,
        };
        let by = arg(cx, 5);
        let by = operand(cx, by)?;
        let set = name_at(cx, 6)?;
        clause.search(Search::new(order, by, set));
    }
    let cycle = arg(cx, 7);
    if !absent(cx, cycle) {
        let by = operand(cx, cycle)?;
        let set = name_at(cx, 8)?;
        let using = name_at(cx, 9)?;
        clause.cycle(Cycle::new(by, set, using));
    }
    Ok(Node::With(clause.into()))
}

/// The WITH clause at `index`.
pub(super) fn clause_at<'cx>(
    cx: &mut FunctionContext<'cx>,
    index: usize,
) -> NeonResult<AnyWithClause> {
    let value = arg(cx, index);
    match node(cx, value) {
        Some(Node::With(clause)) => Ok(clause),
        _ => refuse(cx, "expected a With clause"),
    }
}
