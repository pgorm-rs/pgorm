use pgorm_sql_macro::sql;

// The first statement is well-formed; the rejection has to come from the second.
const QUERIES: &str = sql!(
    "SELECT id FROM cake;
     SELECT id FROM WHERE cake;"
);

fn main() {
    let _ = QUERIES;
}
