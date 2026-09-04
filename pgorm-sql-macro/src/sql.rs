//! The `sql!` half: hold a literal's text to the grammar and report refusals.

use syn::{Error, LitStr};

use crate::oracle;

/// Hold the literal's text to the grammar.
///
/// `pg_query::parse` walks a whole `;`-separated script in one pass and stops at
/// the first statement the grammar refuses, so multi-statement literals need no
/// splitting of their own.
// [spec:pgorm:sem:macros.sql.script]
// [spec:pgorm:req:macros.sql.ceiling]    syntax only: the parser has no catalog
pub(crate) fn validate(literal: &LitStr) -> Result<(), Error> {
    let sql = literal.value();

    match pg_query::parse(&sql) {
        Ok(_) => Ok(()),
        Err(error) => Err(rejection(literal, &sql, &oracle::parser_message(&error))),
    }
}

/// Build the compile error for a rejected literal.
///
/// The span is the whole literal: pointing at the offending byte inside it would
/// need `proc_macro::Literal::subspan`, which is nightly-only, so the position
/// rides in the message as a caret under the offending line instead.
// [spec:pgorm:req:macros.sql.reject]    a grammar rejection, spanned on the literal
// [spec:pgorm:req:macros.sql.ceiling]    the span is the whole literal
fn rejection(literal: &LitStr, sql: &str, message: &str) -> Error {
    let mut text = format!("PostgreSQL rejected this SQL: {message}");
    if let Some(block) =
        oracle::token_offset(sql, message).and_then(|offset| oracle::caret_block(sql, offset))
    {
        text.push_str(&block);
    }

    Error::new(literal.span(), text)
}
