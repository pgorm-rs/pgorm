//! The FROM items that read a relation through something more than its
//! name: a set-returning function with an ordinal column, column names, a
//! column definition list or `ROWS FROM`, and a table read through
//! `TABLESAMPLE`.

use super::*;

impl QueryBuilder {
    /// Translate a [`TableFunction`] FROM item: `[ROWS FROM (]f(..)[ AS (..)]
    /// [, g(..)])] [WITH ORDINALITY] AS "alias"[("a", "b")]`, a lone call's
    /// column definition list standing where the column names would.
    // [spec:pgorm:req:sql.render.table-function+2]
    // [spec:pgorm:req:sql.render.column-definitions]
    pub(super) fn prepare_table_function(
        &self,
        function: &TableFunction,
        alias: &Name,
        sql: &mut dyn SqlWriter,
    ) {
        let rows_from = function.spelled_rows_from();
        if rows_from {
            write!(sql, "ROWS FROM (").unwrap();
        }
        for (i, call) in function.functions.iter().enumerate() {
            if i != 0 {
                write!(sql, ", ").unwrap();
            }
            self.prepare_function_name(&call.call.func, sql);
            self.prepare_function_arguments(&call.call, sql);
            if rows_from && !call.definitions.is_empty() {
                write!(sql, " AS ").unwrap();
                self.prepare_column_definitions(&call.definitions, sql);
            }
        }
        if rows_from {
            write!(sql, ")").unwrap();
        }
        if function.ordinality {
            write!(sql, " WITH ORDINALITY").unwrap();
        }
        write!(sql, " AS ").unwrap();
        alias.prepare(sql.as_writer());
        match function.functions.as_slice() {
            [lone] if !rows_from && !lone.definitions.is_empty() => {
                self.prepare_column_definitions(&lone.definitions, sql);
            }
            _ if !function.columns.is_empty() => {
                write!(sql, "(").unwrap();
                for (i, column) in function.columns.iter().enumerate() {
                    if i != 0 {
                        write!(sql, ", ").unwrap();
                    }
                    column.prepare(sql.as_writer());
                }
                write!(sql, ")").unwrap();
            }
            _ => {}
        }
    }

    /// Translate a column definition list, `("a" integer, "b" text)`, each
    /// type written as a column's is.
    // [spec:pgorm:req:sql.render.column-definitions]
    fn prepare_column_definitions(
        &self,
        definitions: &[(Name, ColumnType)],
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "(").unwrap();
        for (i, (name, column_type)) in definitions.iter().enumerate() {
            if i != 0 {
                write!(sql, ", ").unwrap();
            }
            name.prepare(sql.as_writer());
            write!(sql, " ").unwrap();
            self.prepare_column_type(column_type, sql);
        }
        write!(sql, ")").unwrap();
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
