use pgorm_sql_macro::prql;

// PRQL refuses a parameterized limit; the refusal lands at build time.
fn main() {
    let _ = prql!("from invoice | take $1", 5_i64);
}
