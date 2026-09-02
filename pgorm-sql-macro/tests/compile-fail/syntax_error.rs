use pgorm_sql_macro::sql;

// `FORM` is not a keyword, so the grammar stops at the table name behind it.
const QUERY: &str = sql!("SELECT id FORM cake");

fn main() {
    let _ = QUERY;
}
