use pgorm_sql_macro::prql;

// An s-string is the escape hatch: prqlc passes its contents through
// untouched, so the oracle is what stands between this and the server.
fn main() {
    let _ = prql!(r#"from invoice | derive x = s"SELEKT !!! (" | take 2"#);
}
