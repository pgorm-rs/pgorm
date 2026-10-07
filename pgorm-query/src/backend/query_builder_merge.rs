//! `MERGE`: the statement and its `WHEN` arms.

use super::*;
use crate::query::{MergeArms, MergeAssignments};

impl QueryBuilder {
    /// Translate [`MergeStatement`] into SQL statement: the WITH prefix,
    /// `MERGE INTO [ONLY ]<target> USING <source> ON <condition>`, then the
    /// matched arms and the not-matched arms, each kind's conditional arms
    /// in the order they were added and its unconditional arm last.
    // [spec:pgorm:req:sql.render.merge+1]
    pub(crate) fn prepare_merge_statement(&self, merge: &MergeStatement, sql: &mut dyn SqlWriter) {
        if let Some(with) = &merge.with {
            self.prepare_plain_with_clause(with, sql);
        }

        write!(sql, "MERGE INTO ").unwrap();
        if merge.only {
            write!(sql, "ONLY ").unwrap();
        }
        self.prepare_named_table(&merge.target, sql);

        write!(sql, " USING ").unwrap();
        self.prepare_from_item(&merge.source, sql);

        write!(sql, " ON ").unwrap();
        self.prepare_condition_where(&merge.on, sql);

        self.prepare_merge_arms("MATCHED", &merge.matched, sql, Self::prepare_matched_action);
        self.prepare_merge_arms(
            "NOT MATCHED",
            &merge.not_matched,
            sql,
            |builder, action, sql| match action {
                NotMatchedAction::Insert(insert) => builder.prepare_merge_insert(insert, sql),
                NotMatchedAction::InsertDefaultValues => {
                    write!(sql, "INSERT DEFAULT VALUES").unwrap()
                }
                NotMatchedAction::DoNothing => write!(sql, "DO NOTHING").unwrap(),
            },
        );
        self.prepare_merge_arms(
            "NOT MATCHED BY SOURCE",
            &merge.not_matched_by_source,
            sql,
            Self::prepare_matched_action,
        );

        self.prepare_returning(
            merge.returning.as_ref(),
            merge.returns_action.then_some("merge_action()"),
            sql,
        );
    }

    /// `UPDATE SET ..`, `DELETE` or `DO NOTHING`: what an arm does to a target
    /// row, matched or not matched by source.
    fn prepare_matched_action(&self, action: &MatchedAction, sql: &mut dyn SqlWriter) {
        match action {
            MatchedAction::Update(update) => {
                write!(sql, "UPDATE SET ").unwrap();
                self.prepare_merge_assignments(&update.sets, sql);
            }
            MatchedAction::Delete => write!(sql, "DELETE").unwrap(),
            MatchedAction::DoNothing => write!(sql, "DO NOTHING").unwrap(),
        }
    }

    /// ` WHEN <kind>[ AND <condition>] THEN <action>` for each arm of one kind:
    /// the conditional arms in order, then the unconditional one.
    fn prepare_merge_arms<A>(
        &self,
        kind: &str,
        arms: &MergeArms<A>,
        sql: &mut dyn SqlWriter,
        action: impl Fn(&Self, &A, &mut dyn SqlWriter),
    ) {
        for (condition, arm) in &arms.conditional {
            write!(sql, " WHEN {kind} AND ").unwrap();
            self.prepare_condition_where(condition, sql);
            write!(sql, " THEN ").unwrap();
            action(self, arm, sql);
        }
        if let Some(arm) = &arms.otherwise {
            write!(sql, " WHEN {kind} THEN ").unwrap();
            action(self, arm, sql);
        }
    }

    /// `"col" = <expr>, ...`, the assignments of an `UPDATE SET`.
    fn prepare_merge_assignments(&self, sets: &MergeAssignments, sql: &mut dyn SqlWriter) {
        std::iter::once(&sets.first)
            .chain(&sets.rest)
            .fold(true, |first, (column, value)| {
                if !first {
                    write!(sql, ", ").unwrap();
                }
                column.prepare(sql.as_writer());
                write!(sql, " = ").unwrap();
                self.prepare_simple_expr(value, sql);
                false
            });
    }

    /// `INSERT ("col", ...)[ OVERRIDING ..] VALUES (<expr>, ...)`: the column
    /// list and the row written from the same pairs, so they line up.
    fn prepare_merge_insert(&self, insert: &MergeInsert, sql: &mut dyn SqlWriter) {
        let pairs = || std::iter::once(&insert.values.first).chain(&insert.values.rest);
        write!(sql, "INSERT (").unwrap();
        pairs().fold(true, |first, (column, _)| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            column.prepare(sql.as_writer());
            false
        });
        write!(sql, ")").unwrap();
        match insert.overriding {
            Some(Overriding::SystemValue) => write!(sql, " OVERRIDING SYSTEM VALUE").unwrap(),
            Some(Overriding::UserValue) => write!(sql, " OVERRIDING USER VALUE").unwrap(),
            None => (),
        }
        write!(sql, " VALUES (").unwrap();
        pairs().fold(true, |first, (_, value)| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            self.prepare_simple_expr(value, sql);
            false
        });
        write!(sql, ")").unwrap();
    }
}
