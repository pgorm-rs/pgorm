//! Compile-time PostgreSQL grammar validation for raw SQL string literals.
//!
//! The crate is a sibling of `pgorm-macros` rather than another macro inside it
//! because validation links libpg_query, the PostgreSQL server's own parser, as
//! a C dependency, and the entity derives have no use for it.
#![warn(missing_docs)]
#![deny(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stderr,
    clippy::print_stdout,
    clippy::todo,
    clippy::unreachable,
    clippy::unwrap_used
)]

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Error, LitStr, parse_macro_input};

/// Hold a raw SQL string literal to the PostgreSQL grammar at compile time.
///
/// The literal is parsed by libpg_query — the PostgreSQL server's own parser —
/// while the crate that wrote it is compiled. A literal the grammar accepts
/// expands to itself, unchanged, as a `&'static str`; a literal the grammar
/// rejects is a compile error on the literal carrying the parser's own message.
///
/// ```
/// use pgorm_sql_macro::sql;
///
/// const RECENT: &str = sql!("SELECT id, name FROM cake WHERE id > $1 ORDER BY id");
/// assert_eq!(RECENT, "SELECT id, name FROM cake WHERE id > $1 ORDER BY id");
///
/// // A `;`-separated script is checked statement by statement.
/// const SETUP: &str = sql!("CREATE TABLE cake (id int); INSERT INTO cake VALUES (1);");
/// assert!(SETUP.contains("INSERT INTO cake"));
/// ```
///
/// `sql!("SELECT * FORM cake")` does not compile: the parser reports
/// `syntax error at or near "cake"`, and the macro reports it back on the
/// literal with a caret under the offending column.
///
/// The check is syntax only. libpg_query carries no catalog, so a well-formed
/// statement against tables and columns that do not exist passes happily — this
/// rules out typos in SQL, not mistakes about the schema.
// [spec:pgorm:def:macros.sql+1]
// [spec:pgorm:req:macros.sql.reject]    non-literal input is `syn`'s own refusal
#[proc_macro]
pub fn sql(input: TokenStream) -> TokenStream {
    let literal = parse_macro_input!(input as LitStr);

    match validate(&literal) {
        Ok(()) => quote!(#literal).into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// Hold the literal's text to the grammar.
///
/// `pg_query::parse` walks a whole `;`-separated script in one pass and stops at
/// the first statement the grammar refuses, so multi-statement literals need no
/// splitting of their own.
// [spec:pgorm:sem:macros.sql.script]
// [spec:pgorm:req:macros.sql.ceiling]    syntax only: the parser has no catalog
fn validate(literal: &LitStr) -> Result<(), Error> {
    let sql = literal.value();

    match pg_query::parse(&sql) {
        Ok(_) => Ok(()),
        Err(error) => Err(rejection(literal, &sql, &parser_message(&error))),
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
    if let Some(block) = token_offset(sql, message).and_then(|offset| caret_block(sql, offset)) {
        text.push_str(&block);
    }

    Error::new(literal.span(), text)
}

/// The line of `sql` holding byte `offset`, with a caret beneath that column.
fn caret_block(sql: &str, offset: usize) -> Option<String> {
    let (before, after) = sql.split_at_checked(offset)?;
    let head = before.get(before.rfind('\n').map_or(0, |index| index + 1)..)?;
    let tail = after.get(..after.find('\n').unwrap_or(after.len()))?;
    let column = " ".repeat(head.chars().count());

    Some(format!("\n\n  {head}{tail}\n  {column}^"))
}

/// Where the parser stopped, as a byte offset into `sql`.
///
/// libpg_query surfaces only the message, not the cursor position, so the offset
/// is recovered from the token the message names — and messages that name no
/// token, such as `syntax error at end of input`, get no caret. Matching the
/// render oracle in `pgorm-query/tests/postgres/oracle.rs`, the last occurrence
/// is taken: a token the parser choked on rarely reappears past the point it
/// gave up.
fn token_offset(sql: &str, message: &str) -> Option<usize> {
    let token = message.split("at or near \"").nth(1)?.strip_suffix('"')?;

    sql.rfind(token)
}

/// The parser's own diagnostic, unwrapped from the `pg_query` error variant that
/// would otherwise prefix it with a stage name.
fn parser_message(error: &pg_query::Error) -> String {
    match error {
        pg_query::Error::Parse(message) => message.clone(),
        other => other.to_string(),
    }
}
