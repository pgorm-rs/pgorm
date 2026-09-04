use pgorm_sql_macro::prql;

// Bare PRQL tokens are not a string literal, so nothing reaches the compiler.
fn main() {
    let _ = prql!(from invoice | take 5);
}
