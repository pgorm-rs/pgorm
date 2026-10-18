//! The FROM items that read a relation through something more than its
//! name: a set-returning function with an ordinal column, column names or
//! `ROWS FROM`, and a table read through `TABLESAMPLE`.

use super::*;

impl QueryBuilder {
    /// Translate a [`TableFunction`] FROM item: `[ROWS FROM (]f(..)[, g(..))]
    /// [WITH ORDINALITY] AS "alias"[("a", "b")]`.
    // [spec:pgorm:req:sql.render.table-function]
    pub(super) fn prepare_table_function(
        &self,
        function: &TableFunction,
        alias: &Name,
        sql: &mut dyn SqlWriter,
    ) {
        if function.rows_from {
            write!(sql, "ROWS FROM (").unwrap();
        }
        for (i, call) in function.functions.iter().enumerate() {
            if i != 0 {
                write!(sql, ", ").unwrap();
            }
            self.prepare_function_name(&call.func, sql);
            self.prepare_function_arguments(call, sql);
        }
        if function.rows_from {
            write!(sql, ")").unwrap();
        }
        if function.ordinality {
            write!(sql, " WITH ORDINALITY").unwrap();
        }
        write!(sql, " AS ").unwrap();
        alias.prepare(sql.as_writer());
        if !function.columns.is_empty() {
            write!(sql, "(").unwrap();
            for (i, column) in function.columns.iter().enumerate() {
                if i != 0 {
                    write!(sql, ", ").unwrap();
                }
                column.prepare(sql.as_writer());
            }
            write!(sql, ")").unwrap();
        }
    }

    /// Translate a [`TableSample`] FROM item: the table and its alias, then
    /// `TABLESAMPLE <method> ($n)` and an optional `REPEATABLE ($m)`. The
    /// alias precedes the clause because the grammar puts it there.
    // [spec:pgorm:req:sql.render.table-sample]
    pub(super) fn prepare_table_sample(&self, sample: &TableSample, sql: &mut dyn SqlWriter) {
        self.prepare_named_table(&sample.table, sql);
        write!(
            sql,
            " TABLESAMPLE {} (",
            match sample.method {
                SampleMethod::Bernoulli => "BERNOULLI",
                SampleMethod::System => "SYSTEM",
            }
        )
        .unwrap();
        sql.push_param(sample.percentage.clone());
        write!(sql, ")").unwrap();
        if let Some(seed) = &sample.seed {
            write!(sql, " REPEATABLE (").unwrap();
            sql.push_param(seed.clone());
            write!(sql, ")").unwrap();
        }
    }
}
