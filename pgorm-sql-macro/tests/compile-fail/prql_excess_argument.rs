use pgorm_sql_macro::prql;

// One placeholder, three arguments: each surplus argument is refused where it
// stands, named after the placeholder the query does not have.
fn main() {
    let _ = prql!("from invoice | filter total > $1", 100_i64, 2_i64, 3_i64);
}
