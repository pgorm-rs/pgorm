//! The plain `WITH` prefix, written for every statement that carries one.

use super::*;

impl QueryBuilder {
    /// `WITH ` and the clause's common table expressions, comma-separated.
    /// `prepare_with_clause` writes a plain clause through this, and a `MERGE`,
    /// which takes no recursive clause, calls it directly.
    // [spec:pgorm:req:sql.render.cte+4]
    pub(super) fn prepare_plain_with_clause(&self, with: &WithClause, sql: &mut dyn SqlWriter) {
        write!(sql, "WITH ").unwrap();
        for (i, cte) in with.ctes().enumerate() {
            if i != 0 {
                write!(sql, ", ").unwrap();
            }
            self.prepare_with_query_clause_common_table(cte, sql);
        }
    }
}
