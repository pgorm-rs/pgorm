//! `JSON_TABLE` as a FROM item.

use super::json::exists_behavior;
use super::*;
use crate::json::{ColumnKind, JsonTableColumn};

impl QueryBuilder {
    /// Translate a [`JsonTable`] FROM item into SQL: `JSON_TABLE(..) AS
    /// "alias"`.
    // [spec:pgorm:req:sql.render.json-table]
    pub(super) fn prepare_json_table(
        &self,
        table: &JsonTable,
        alias: &Name,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "JSON_TABLE(").unwrap();
        self.prepare_json_input(&table.target.context, sql);
        write!(sql, ", ").unwrap();
        self.prepare_json_path_literal(&table.target.path, sql);
        if let Some(name) = &table.path_name {
            write!(sql, " AS ").unwrap();
            name.prepare(sql.as_writer());
        }
        self.prepare_json_passing(&table.target.passing, sql);
        write!(sql, " COLUMNS (").unwrap();
        self.prepare_json_table_columns(&table.columns, sql);
        write!(sql, ")").unwrap();
        match table.on_error {
            Some(JsonTableBehavior::Error) => write!(sql, " ERROR ON ERROR").unwrap(),
            Some(JsonTableBehavior::Empty) => write!(sql, " EMPTY ON ERROR").unwrap(),
            None => {}
        }
        write!(sql, ") AS ").unwrap();
        alias.prepare(sql.as_writer());
    }

    fn prepare_json_table_columns(&self, columns: &[JsonTableColumn], sql: &mut dyn SqlWriter) {
        for (i, column) in columns.iter().enumerate() {
            if i != 0 {
                write!(sql, ", ").unwrap();
            }
            match &column.0 {
                ColumnKind::Ordinality(name) => {
                    name.prepare(sql.as_writer());
                    write!(sql, " FOR ORDINALITY").unwrap();
                }
                ColumnKind::Value(column) => {
                    self.prepare_json_column_head(&column.name, &column.column_type, sql);
                    self.prepare_json_column_path(column.path.as_deref(), sql);
                    for (behavior, on) in [(&column.on_empty, "EMPTY"), (&column.on_error, "ERROR")]
                    {
                        if let Some(behavior) = behavior {
                            write!(sql, " ").unwrap();
                            self.prepare_json_value_behavior(behavior, sql);
                            write!(sql, " ON {on}").unwrap();
                        }
                    }
                }
                ColumnKind::Query(column) => {
                    self.prepare_json_column_head(&column.name, &column.column_type, sql);
                    write!(sql, " FORMAT JSON").unwrap();
                    self.prepare_json_column_path(column.path.as_deref(), sql);
                    self.prepare_json_shaping(column.shaping, sql);
                    for (behavior, on) in [(&column.on_empty, "EMPTY"), (&column.on_error, "ERROR")]
                    {
                        if let Some(behavior) = behavior {
                            write!(sql, " ").unwrap();
                            self.prepare_json_query_behavior(behavior, sql);
                            write!(sql, " ON {on}").unwrap();
                        }
                    }
                }
                ColumnKind::Exists(column) => {
                    self.prepare_json_column_head(&column.name, &column.column_type, sql);
                    write!(sql, " EXISTS").unwrap();
                    self.prepare_json_column_path(column.path.as_deref(), sql);
                    if let Some(behavior) = column.on_error {
                        write!(sql, " {} ON ERROR", exists_behavior(behavior)).unwrap();
                    }
                }
                ColumnKind::Nested(nested) => {
                    write!(sql, "NESTED PATH ").unwrap();
                    self.prepare_json_path_literal(&nested.path, sql);
                    if let Some(name) = &nested.path_name {
                        write!(sql, " AS ").unwrap();
                        name.prepare(sql.as_writer());
                    }
                    write!(sql, " COLUMNS (").unwrap();
                    self.prepare_json_table_columns(&nested.columns, sql);
                    write!(sql, ")").unwrap();
                }
            }
        }
    }

    fn prepare_json_column_head(
        &self,
        name: &Name,
        column_type: &ColumnType,
        sql: &mut dyn SqlWriter,
    ) {
        name.prepare(sql.as_writer());
        write!(sql, " ").unwrap();
        self.prepare_column_type(column_type, sql);
    }

    fn prepare_json_column_path(&self, path: Option<&str>, sql: &mut dyn SqlWriter) {
        if let Some(path) = path {
            write!(sql, " PATH ").unwrap();
            self.prepare_json_path_literal(path, sql);
        }
    }

    /// A path the grammar takes only as a string constant, written as the
    /// value pipeline writes an inlined string in both render paths: a quote
    /// or a backslash escaped by a backslash inside `E'..'`, any other path a
    /// plain `'..'`.
    // [spec:pgorm:req:sql.render.json-table]
    fn prepare_json_path_literal(&self, path: &str, sql: &mut dyn SqlWriter) {
        let mut literal = String::new();
        self.write_string_quoted(path, &mut literal);
        write!(sql, "{literal}").unwrap();
    }
}
