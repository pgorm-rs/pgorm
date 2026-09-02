use pgorm_sql_macro::sql;

// A literal, but not one that could hold SQL.
const QUERY: &str = sql!(42);

fn main() {
    let _ = QUERY;
}
