//! MERGE over pgorm-query's typestate: `merge()` gives a pending MERGE, which
//! has no WHEN arm and so nothing to run, and its first arm gives the
//! statement. Each arm's action is checked against the kind of row the arm
//! takes, as the Rust types check it.

use neon::prelude::*;
use pgorm::pgorm_query::{
    AnyWithClause, MatchedAction, MergeInsert, MergeStatement, MergeUpdate, NotMatchedAction,
    Overriding, PendingMerge, Query,
};

use super::{
    Node,
    args::{absent, arg, choice, name_at, operand_at, predicate, refuse, this},
    returning, table, with,
};

pub(super) const EXPORTS: &[(&str, super::Build)] = &[
    ("mergeNew", merge_new),
    ("mergeWhen", merge_when),
    ("mergeReturning", merge_returning),
    ("mergeReturningAction", merge_returning_action),
    ("mergeWith", merge_with),
    ("mergeOnly", merge_only),
    ("mergeActionUpdate", action_update),
    ("mergeActionInsert", action_insert),
    ("mergeActionSet", action_set),
    ("mergeActionOverriding", action_overriding),
    ("mergeActionKeyword", action_keyword),
];

/// An action an arm takes, before it is matched to the arm's kind of row.
#[derive(Debug, Clone)]
pub(crate) enum Action {
    Update(MergeUpdate),
    Insert(MergeInsert),
    Delete,
    DoNothing,
    InsertDefaults,
}

impl Action {
    fn describe(&self) -> &'static str {
        match self {
            Self::Update(_) => "an update",
            Self::Insert(_) => "an insert",
            Self::Delete => "a delete",
            Self::DoNothing => "doing nothing",
            Self::InsertDefaults => "an insert of defaults",
        }
    }
}

/// `mergeNew(target, source, on)`: `MERGE INTO target USING source ON ..`,
/// pending until its first arm.
// [spec:pgorm:req:napi.merge]
fn merge_new(cx: &mut FunctionContext) -> NeonResult<Node> {
    let target = match this(cx, 0)? {
        Node::Table(table) => table,
        other => {
            return refuse(
                cx,
                format!("a MERGE's target is a Table, not {}", other.describe()),
            );
        }
    };
    let source = arg(cx, 1);
    let source = table::from_item(cx, source)?;
    let on = arg(cx, 2);
    let on = predicate(cx, on)?;
    Ok(Node::PendingMerge(Box::new(Query::merge(
        target, source, on,
    ))))
}

/// The action at `index` as the arm `kind` takes it. A target row is
/// updated, deleted or left alone; a source row is inserted or skipped.
fn arm_action(
    cx: &mut FunctionContext,
    kind: &str,
    index: usize,
) -> NeonResult<Result<MatchedAction, NotMatchedAction>> {
    let action = match this(cx, index)? {
        Node::MergeAction(action) => action,
        other => {
            return refuse(
                cx,
                format!("an arm takes a MergeAction, not {}", other.describe()),
            );
        }
    };
    let target_row = kind != "notMatched";
    Ok(match (target_row, action) {
        (true, Action::Update(update)) => Ok(update.into()),
        (true, Action::Delete) => Ok(MatchedAction::Delete),
        (true, Action::DoNothing) => Ok(MatchedAction::DoNothing),
        (false, Action::Insert(insert)) => Err(insert.into()),
        (false, Action::InsertDefaults) => Err(NotMatchedAction::InsertDefaultValues),
        (false, Action::DoNothing) => Err(NotMatchedAction::DoNothing),
        (true, other) => {
            let what = other.describe();
            return refuse(
                cx,
                format!("an arm on a target row updates, deletes or does nothing, not {what}"),
            );
        }
        (false, other) => {
            let what = other.describe();
            return refuse(
                cx,
                format!("a NOT MATCHED arm inserts or does nothing, not {what}"),
            );
        }
    })
}

/// `mergeWhen(merge, kind, action, condition)`: one more WHEN arm, the first
/// making a pending MERGE a statement.
// [spec:pgorm:req:napi.merge]
fn merge_when(cx: &mut FunctionContext) -> NeonResult<Node> {
    let node = this(cx, 0)?;
    let kind = choice(
        cx,
        1,
        "an arm's kind",
        &["matched", "notMatched", "notMatchedBySource"],
    )?;
    let action = arm_action(cx, kind, 2)?;
    let condition = arg(cx, 3);
    let condition = if absent(cx, condition) {
        None
    } else {
        Some(predicate(cx, condition)?)
    };
    let merge = match node {
        Node::PendingMerge(pending) => Box::new(first_arm(*pending, kind, action, condition)),
        Node::Merge(mut merge) => {
            next_arm(&mut merge, kind, action, condition);
            merge
        }
        other => return refuse(cx, format!("expected a MERGE, got {}", other.describe())),
    };
    Ok(Node::Merge(merge))
}

type Arm = Result<MatchedAction, NotMatchedAction>;

fn first_arm(
    pending: PendingMerge,
    kind: &str,
    action: Arm,
    condition: Option<pgorm::pgorm_query::Condition>,
) -> MergeStatement {
    match (kind, action, condition) {
        ("matched", Ok(action), None) => pending.when_matched(action),
        ("matched", Ok(action), Some(condition)) => pending.when_matched_and(condition, action),
        (_, Ok(action), None) => pending.when_not_matched_by_source(action),
        (_, Ok(action), Some(condition)) => {
            pending.when_not_matched_by_source_and(condition, action)
        }
        (_, Err(action), None) => pending.when_not_matched(action),
        (_, Err(action), Some(condition)) => pending.when_not_matched_and(condition, action),
    }
}

fn next_arm(
    merge: &mut MergeStatement,
    kind: &str,
    action: Arm,
    condition: Option<pgorm::pgorm_query::Condition>,
) {
    match (kind, action, condition) {
        ("matched", Ok(action), None) => merge.when_matched(action),
        ("matched", Ok(action), Some(condition)) => merge.when_matched_and(condition, action),
        (_, Ok(action), None) => merge.when_not_matched_by_source(action),
        (_, Ok(action), Some(condition)) => merge.when_not_matched_by_source_and(condition, action),
        (_, Err(action), None) => merge.when_not_matched(action),
        (_, Err(action), Some(condition)) => merge.when_not_matched_and(condition, action),
    };
}

fn statement(cx: &mut FunctionContext) -> NeonResult<Box<MergeStatement>> {
    match this(cx, 0)? {
        Node::Merge(merge) => Ok(merge),
        Node::PendingMerge(_) => refuse(cx, "a MERGE needs a WHEN arm first"),
        other => refuse(cx, format!("expected a MERGE, got {}", other.describe())),
    }
}

/// `mergeReturning(merge, items, oldAs, newAs)`.
fn merge_returning(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut merge = statement(cx)?;
    let clause = returning::clause(cx, 1)?;
    merge.returning(clause);
    Ok(Node::Merge(merge))
}

/// `mergeReturningAction(merge)`: `merge_action()` first in the RETURNING
/// list, the one place PostgreSQL resolves it.
fn merge_returning_action(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut merge = statement(cx)?;
    merge.returning_action();
    Ok(Node::Merge(merge))
}

/// `mergeWith(merge, clause)`: a plain WITH clause; PostgreSQL refuses
/// `WITH RECURSIVE` before a MERGE.
fn merge_with(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut merge = statement(cx)?;
    match with::clause_at(cx, 1)? {
        AnyWithClause::Plain(clause) => merge.with(clause),
        AnyWithClause::Recursive(_) => {
            return refuse(cx, "PostgreSQL takes no WITH RECURSIVE before a MERGE");
        }
    };
    Ok(Node::Merge(merge))
}

/// `mergeOnly(merge)`: `ONLY` before the target, leaving inheriting tables
/// alone.
fn merge_only(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut merge = statement(cx)?;
    merge.only();
    Ok(Node::Merge(merge))
}

/// `mergeActionUpdate(column, value)`: `UPDATE SET column = value`, its first
/// assignment.
// [spec:pgorm:req:napi.merge]
fn action_update(cx: &mut FunctionContext) -> NeonResult<Node> {
    let column = name_at(cx, 0)?;
    let value = operand_at(cx, 1)?;
    Ok(Node::MergeAction(Action::Update(MergeUpdate::value(
        column, value,
    ))))
}

/// `mergeActionInsert(column, value)`: `INSERT (column) VALUES (value)`, its
/// first column.
fn action_insert(cx: &mut FunctionContext) -> NeonResult<Node> {
    let column = name_at(cx, 0)?;
    let value = operand_at(cx, 1)?;
    Ok(Node::MergeAction(Action::Insert(MergeInsert::value(
        column, value,
    ))))
}

/// `mergeActionSet(action, column, value)`: one more assignment or column.
fn action_set(cx: &mut FunctionContext) -> NeonResult<Node> {
    let action = this(cx, 0)?;
    let column = name_at(cx, 1)?;
    let value = operand_at(cx, 2)?;
    let action = match action {
        Node::MergeAction(Action::Update(update)) => {
            Action::Update(update.and_value(column, value))
        }
        Node::MergeAction(Action::Insert(insert)) => {
            Action::Insert(insert.and_value(column, value))
        }
        other => {
            return refuse(
                cx,
                format!(
                    "expected a MERGE update or insert, got {}",
                    other.describe()
                ),
            );
        }
    };
    Ok(Node::MergeAction(action))
}

/// `mergeActionOverriding(insert, which)`: `OVERRIDING SYSTEM VALUE` or
/// `USER VALUE` on an insert.
fn action_overriding(cx: &mut FunctionContext) -> NeonResult<Node> {
    let Node::MergeAction(Action::Insert(insert)) = this(cx, 0)? else {
        return refuse(cx, "overriding applies to a MERGE insert");
    };
    let overriding = match choice(cx, 1, "overriding", &["systemValue", "userValue"])? {
        "systemValue" => Overriding::SystemValue,
        _ => Overriding::UserValue,
    };
    Ok(Node::MergeAction(Action::Insert(
        insert.overriding(overriding),
    )))
}

/// `mergeActionKeyword(which)`: `DELETE`, `DO NOTHING` or `INSERT DEFAULT
/// VALUES`.
fn action_keyword(cx: &mut FunctionContext) -> NeonResult<Node> {
    let action = match choice(
        cx,
        0,
        "an action",
        &["delete", "doNothing", "insertDefaults"],
    )? {
        "delete" => Action::Delete,
        "doNothing" => Action::DoNothing,
        _ => Action::InsertDefaults,
    };
    Ok(Node::MergeAction(action))
}
