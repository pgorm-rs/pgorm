//! A SELECT's set operations: the arms after its own query, each a
//! parenthesised SELECT after its operator, grouped so that the chain means
//! what it reads in call order.

use super::*;

/// Whether a set-operation arm appended with `next`, right after one appended
/// with `previous`, must close a parenthesis around everything written before
/// it. A chain means what it reads left to right, and PostgreSQL agrees except
/// in one place: `INTERSECT` binds tighter than `UNION` and `EXCEPT`, which
/// share a level and associate left, so an `INTERSECT` arm written flat after
/// a `UNION` or `EXCEPT` arm would take only that arm as its left operand.
/// After the parenthesis the run before it is one operand, so only the arm
/// immediately before decides.
// [spec:pgorm:sem:sql.ast.select.union+2]
fn regroups(previous: UnionType, next: UnionType) -> bool {
    let intersects = |op| matches!(op, UnionType::Intersect | UnionType::IntersectAll);
    intersects(next) && !intersects(previous)
}

impl QueryBuilder {
    /// Open one parenthesis for each arm that will close one, before the
    /// statement's `SELECT` and after its WITH clause, which stays outside
    /// them.
    // [spec:pgorm:req:sql.render.select-order+7]
    pub(super) fn open_set_operation_groups(
        &self,
        unions: &[(UnionType, SelectStatement)],
        sql: &mut dyn SqlWriter,
    ) {
        let groups = unions
            .windows(2)
            .filter(|pair| regroups(pair[0].0, pair[1].0))
            .count();
        for _ in 0..groups {
            write!(sql, "(").unwrap();
        }
    }

    /// Translate the arms: ` <operator> (<select>)` each, an arm that
    /// regroups the chain closing a parenthesis before its operator.
    // [spec:pgorm:req:sql.render.select-order+7]
    pub(super) fn prepare_set_operations(
        &self,
        unions: &[(UnionType, SelectStatement)],
        sql: &mut dyn SqlWriter,
    ) {
        let mut previous = None;
        for (union_type, query) in unions {
            if previous.is_some_and(|previous| regroups(previous, *union_type)) {
                write!(sql, ")").unwrap();
            }
            let operator = match union_type {
                UnionType::Distinct => "UNION",
                UnionType::All => "UNION ALL",
                UnionType::Intersect => "INTERSECT",
                UnionType::IntersectAll => "INTERSECT ALL",
                UnionType::Except => "EXCEPT",
                UnionType::ExceptAll => "EXCEPT ALL",
            };
            write!(sql, " {operator} (").unwrap();
            self.prepare_select_statement(query, sql);
            write!(sql, ")").unwrap();
            previous = Some(*union_type);
        }
    }
}
