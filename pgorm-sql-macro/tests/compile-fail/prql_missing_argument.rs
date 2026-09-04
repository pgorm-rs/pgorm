use pgorm_sql_macro::prql;

// Two placeholders, one argument: the unbound ones are named.
fn main() {
    let _ = prql!(
        "from invoice | filter total > $1 | filter billing_city == $2",
        100_i64,
    );
}
