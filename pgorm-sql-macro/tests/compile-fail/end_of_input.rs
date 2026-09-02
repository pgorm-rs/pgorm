use pgorm_sql_macro::sql;

// The parser names no token when it runs out of input, so there is no caret to
// place and the message stands on its own.
const QUERY: &str = sql!("SELECT id FROM");

fn main() {
    let _ = QUERY;
}
