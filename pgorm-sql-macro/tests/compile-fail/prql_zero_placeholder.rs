use pgorm_sql_macro::prql;

// The grammar tolerates $0, but no bind protocol can supply it.
fn main() {
    let _ = prql!("from invoice | filter total > $0", 1_i64);
}
