use pgorm_sql_macro::sql;

// A literal the parser cannot even be handed: it never reaches the grammar, and
// the macro still has to answer with an error rather than a panic.
const QUERY: &str = sql!("SELECT id FROM cake\0");

fn main() {
    let _ = QUERY;
}
