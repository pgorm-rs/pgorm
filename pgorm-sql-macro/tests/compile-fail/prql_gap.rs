use pgorm_sql_macro::prql;

// $1 and $3 with no $2: refused before arity, so the two arguments given
// never get the chance to misalign silently.
fn main() {
    let _ = prql!(
        "from invoice | filter total > $1 | filter subtotal < $3",
        1_i64,
        2_i64,
    );
}
