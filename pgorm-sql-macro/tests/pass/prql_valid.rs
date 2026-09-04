use pgorm_sql_macro::prql;

fn main() {
    // The finished SQL and the bound value, in one expansion.
    let (sql, values) = prql!("from invoice | filter total > $1 | take 5", 100_i64);
    assert_eq!(sql, "SELECT * FROM invoice WHERE total > $1 LIMIT 5");
    assert_eq!(values.0.len(), 1);

    // A reused placeholder is one argument.
    let (sql, values) = prql!(
        "from invoice | filter total > $1 | filter subtotal < $1",
        7_i64,
    );
    assert_eq!(sql.matches("$1").count(), 2);
    assert_eq!(values.0.len(), 1);

    // No placeholders, no arguments.
    let (sql, values) = prql!("from invoice | take 3");
    assert_eq!(sql, "SELECT * FROM invoice LIMIT 3");
    assert!(values.0.is_empty());

    // An s-string that lands in well-formed SQL passes the oracle.
    let (sql, _) = prql!(r#"from invoice | derive city = s"lower(billing_city)" | take 1"#);
    assert!(sql.contains("lower(billing_city)"));
}
