use pgorm_sql_macro::prql;

// Grouping an undeclared relation by `this` groups by columns prqlc cannot
// name, which PostgreSQL reads as a whole-row key. Selecting the star beside
// it reads those columns outside an aggregate, which PostgreSQL refuses at
// execution (42803), so prqlc refuses the query here instead.
fn main() {
    let _ = prql!("from invoice | group this (aggregate {n = count this})");
}
