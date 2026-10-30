//! Grouping elements: the items of a `GROUP BY` list beyond a bare
//! expression — a parenthesised set, `()`, `ROLLUP`, `CUBE` and
//! `GROUPING SETS` — over pgorm-query's `GroupingElement`.

use neon::prelude::*;
use pgorm::pgorm_query::GroupingElement;

use super::{
    Node,
    args::{choice, expressions_at, list, node, refuse},
};

pub(super) const EXPORTS: &[(&str, super::Build)] = &[
    ("groupingNew", grouping_new),
    ("groupingSets", grouping_sets),
];

/// A `GROUP BY` item: a grouping element, or an expression, which is the set
/// of that one expression and renders bare, as `add_group_by` writes it.
// [spec:pgorm:req:bindings.select]
pub(super) fn element<'cx>(
    cx: &mut Cx<'cx>,
    value: Handle<'cx, JsValue>,
) -> NeonResult<GroupingElement> {
    match node(cx, value) {
        Some(Node::GroupingElement(element)) => Ok(element),
        Some(Node::Expr(expr)) => Ok(GroupingElement::set([expr])),
        Some(other) => {
            let what = other.describe();
            cx.throw_type_error(format!(
                "a grouping item is an expression or a grouping element, not {what}"
            ))
        }
        None => cx.throw_type_error(
            "a grouping item is an expression or a grouping element, never a bound value",
        ),
    }
}

/// `groupingNew("set" | "rollup" | "cube", expressions)`: one set,
/// `("a", "b")` — `()` with no expressions — or `ROLLUP` or `CUBE` over the
/// expressions, each of which may be a `tuple` standing as one unit.
// [spec:pgorm:req:bindings.select]
fn grouping_new(cx: &mut FunctionContext) -> NeonResult<Node> {
    let kind = choice(cx, 0, "a grouping element", &["set", "rollup", "cube"])?;
    let expressions = expressions_at(cx, 1)?;
    let element = match kind {
        "set" => GroupingElement::set(expressions),
        "rollup" => GroupingElement::rollup(expressions),
        _ => GroupingElement::cube(expressions),
    };
    Ok(Node::GroupingElement(element))
}

/// `groupingSets(items)`: `GROUPING SETS (..)` over at least one item, each a
/// grouping element or an expression, as pgorm-query's builder takes its
/// first element before any other.
// [spec:pgorm:req:bindings.select]
fn grouping_sets(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut items = list(cx, 0)?.into_iter();
    let Some(first) = items.next() else {
        return refuse(cx, "GROUPING SETS takes at least one element");
    };
    let first = element(cx, first)?;
    let mut sets = GroupingElement::sets(first);
    for item in items {
        sets = sets.add(element(cx, item)?);
    }
    Ok(Node::GroupingElement(sets.into()))
}
