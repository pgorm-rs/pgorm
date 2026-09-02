use pgorm_sql_macro::sql;

// The macro takes a literal and nothing else; parameters are bound at the call.
const QUERY: &str = sql!("SELECT id FROM cake WHERE id = $1", 1);

fn main() {
    let _ = QUERY;
}
