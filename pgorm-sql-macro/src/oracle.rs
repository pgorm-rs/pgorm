//! Shared plumbing for reporting a libpg_query rejection.
//!
//! Both macros end at the same oracle — `pg_query::parse` — and both want its
//! diagnostic pointed at the right column of the offending SQL. The helpers
//! here recover that column and render the caret line; whose SQL it was (the
//! caller's literal, or the text prqlc emitted) is the caller's framing.

/// The line of `sql` holding byte `offset`, with a caret beneath that column.
pub(crate) fn caret_block(sql: &str, offset: usize) -> Option<String> {
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
pub(crate) fn token_offset(sql: &str, message: &str) -> Option<usize> {
    let token = message.split("at or near \"").nth(1)?.strip_suffix('"')?;

    sql.rfind(token)
}

/// The parser's own diagnostic, unwrapped from the `pg_query` error variant that
/// would otherwise prefix it with a stage name.
pub(crate) fn parser_message(error: &pg_query::Error) -> String {
    match error {
        pg_query::Error::Parse(message) => message.clone(),
        other => other.to_string(),
    }
}
