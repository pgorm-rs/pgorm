//! `RETURNING`: the list a write statement ends with, and the names it reads
//! a written row's two versions by.

use super::*;

impl QueryBuilder {
    /// ` RETURNING [WITH (OLD AS .., NEW AS ..)] [<lead>, ]<list>`, when the
    /// statement returns anything. `lead` is an item the statement itself
    /// writes ahead of the caller's list, MERGE's `merge_action()`, and the
    /// clause is written when either is present.
    // [spec:pgorm:req:sql.render.returning+3]
    pub(crate) fn prepare_returning(
        &self,
        returning: Option<&ReturningClause>,
        lead: Option<&str>,
        sql: &mut dyn SqlWriter,
    ) {
        if returning.is_none() && lead.is_none() {
            return;
        }
        write!(sql, " RETURNING ").unwrap();
        if let Some(returning) = returning {
            let renames = [("OLD", &returning.old), ("NEW", &returning.new)];
            let mut renames = renames
                .iter()
                .filter_map(|(row, name)| name.as_ref().map(|name| (row, name)))
                .peekable();
            if renames.peek().is_some() {
                write!(sql, "WITH (").unwrap();
                renames.fold(true, |first, (row, name)| {
                    if !first {
                        write!(sql, ", ").unwrap();
                    }
                    write!(sql, "{row} AS ").unwrap();
                    name.prepare(sql.as_writer());
                    false
                });
                write!(sql, ") ").unwrap();
            }
        }
        if let Some(lead) = lead {
            write!(sql, "{lead}").unwrap();
            if returning.is_some() {
                write!(sql, ", ").unwrap();
            }
        }
        let Some(returning) = returning else {
            return;
        };
        match &returning.items {
            ReturningItems::All => write!(sql, "*").unwrap(),
            ReturningItems::Columns(cols) => {
                cols.iter().fold(true, |first, column_ref| {
                    if !first {
                        write!(sql, ", ").unwrap()
                    }
                    self.prepare_column_ref(column_ref, sql);
                    false
                });
            }
            ReturningItems::Exprs(exprs) => {
                exprs.iter().fold(true, |first, expr| {
                    if !first {
                        write!(sql, ", ").unwrap()
                    }
                    self.prepare_simple_expr(expr, sql);
                    false
                });
            }
        }
    }
}
