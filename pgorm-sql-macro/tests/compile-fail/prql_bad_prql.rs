use pgorm_sql_macro::prql;

// `fliter` is not a transform, so prqlc's resolver refuses the query.
fn main() {
    let _ = prql!("from invoice | fliter total > $1", 5_i64);
}
