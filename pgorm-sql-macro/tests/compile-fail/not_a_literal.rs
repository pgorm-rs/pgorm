use pgorm_sql_macro::sql;

// Bare SQL tokens are not a string literal, so nothing reaches the parser.
const QUERY: &str = sql!(SELECT id FROM cake);

fn main() {
    let _ = QUERY;
}
