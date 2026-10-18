//! `COPY … FROM STDIN` and `COPY … TO STDOUT`.

use super::*;
use crate::copy::{CopyColumns, CopyHeader, CopyOptions, Source, format_keyword};

impl QueryBuilder {
    // [spec:pgorm:req:sql.render.copy]
    pub(crate) fn prepare_copy_from_statement<F: CopyFormat>(
        &self,
        copy: &CopyFromStatement<F>,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "COPY ").unwrap();
        self.prepare_table_name(&copy.table, sql);
        if !copy.columns.is_empty() {
            write!(sql, " ").unwrap();
            self.prepare_copy_name_list(&copy.columns, sql);
        }
        write!(sql, " FROM STDIN").unwrap();
        self.prepare_copy_options(format_keyword(&copy.format), &copy.options, sql);
        self.prepare_condition(&copy.r#where, "WHERE", sql);
    }

    // [spec:pgorm:req:sql.render.copy]
    pub(crate) fn prepare_copy_to_statement<F: CopyFormat>(
        &self,
        copy: &CopyToStatement<F>,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "COPY ").unwrap();
        match &copy.source.0 {
            Source::Table(table) => self.prepare_table_name(table, sql),
            Source::Query(query) => {
                write!(sql, "(").unwrap();
                query.prepare_statement(sql);
                write!(sql, ")").unwrap();
            }
        }
        write!(sql, " TO STDOUT").unwrap();
        self.prepare_copy_options(format_keyword(&copy.format), &copy.options, sql);
    }

    /// ` WITH (FORMAT <format>, …)`, every option in the order PostgreSQL's
    /// documentation lists them and each with its value written out. `FORMAT`
    /// is always written, so the statement names its format rather than
    /// leaving it to the server's default.
    fn prepare_copy_options(&self, format: &str, options: &CopyOptions, sql: &mut dyn SqlWriter) {
        write!(sql, " WITH (FORMAT {format}").unwrap();
        if options.freeze {
            write!(sql, ", FREEZE true").unwrap();
        }
        if let Some(delimiter) = options.delimiter {
            write!(sql, ", DELIMITER ").unwrap();
            self.prepare_copy_string(delimiter.encode_utf8(&mut [0; 4]), sql);
        }
        if let Some(null) = &options.null {
            write!(sql, ", NULL ").unwrap();
            self.prepare_copy_string(null, sql);
        }
        if let Some(default) = &options.default {
            write!(sql, ", DEFAULT ").unwrap();
            self.prepare_copy_string(default, sql);
        }
        match options.header {
            CopyHeader::Absent => {}
            CopyHeader::Present => write!(sql, ", HEADER true").unwrap(),
            CopyHeader::Match => write!(sql, ", HEADER match").unwrap(),
        }
        if let Some(quote) = options.quote {
            write!(sql, ", QUOTE ").unwrap();
            self.prepare_copy_string(quote.encode_utf8(&mut [0; 4]), sql);
        }
        if let Some(escape) = options.escape {
            write!(sql, ", ESCAPE ").unwrap();
            self.prepare_copy_string(escape.encode_utf8(&mut [0; 4]), sql);
        }
        self.prepare_copy_forced("FORCE_QUOTE", options.force_quote.as_ref(), sql);
        self.prepare_copy_forced("FORCE_NOT_NULL", options.force_not_null.as_ref(), sql);
        self.prepare_copy_forced("FORCE_NULL", options.force_null.as_ref(), sql);
        if let Some(ignore) = &options.on_error {
            write!(sql, ", ON_ERROR ignore").unwrap();
            if let Some(limit) = ignore.reject_limit {
                write!(sql, ", REJECT_LIMIT {limit}").unwrap();
            }
        }
        if let Some(encoding) = &options.encoding {
            write!(sql, ", ENCODING ").unwrap();
            self.prepare_copy_string(encoding, sql);
        }
        if let Some(verbosity) = options.on_error.and_then(|ignore| ignore.log_verbosity) {
            let verbosity = match verbosity {
                CopyLogVerbosity::Verbose => "verbose",
                CopyLogVerbosity::Silent => "silent",
            };
            write!(sql, ", LOG_VERBOSITY {verbosity}").unwrap();
        }
        write!(sql, ")").unwrap();
    }

    fn prepare_copy_forced(
        &self,
        option: &str,
        columns: Option<&CopyColumns>,
        sql: &mut dyn SqlWriter,
    ) {
        match columns {
            None => {}
            Some(CopyColumns::All) => write!(sql, ", {option} *").unwrap(),
            Some(CopyColumns::Named(names)) => {
                write!(sql, ", {option} ").unwrap();
                self.prepare_copy_name_list(names, sql);
            }
        }
    }

    fn prepare_copy_name_list(&self, names: &[Name], sql: &mut dyn SqlWriter) {
        write!(sql, "(").unwrap();
        names.iter().fold(true, |first, name| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            name.prepare(sql.as_writer());
            false
        });
        write!(sql, ")").unwrap();
    }

    /// An option's string argument, which the grammar takes only as a string
    /// constant: escaped as an inlined value is, and an `E'..'` string when
    /// that escaping wrote a backslash.
    fn prepare_copy_string(&self, value: &str, sql: &mut dyn SqlWriter) {
        let mut literal = String::new();
        self.write_string_quoted(value, &mut literal);
        write!(sql, "{literal}").unwrap();
    }
}
