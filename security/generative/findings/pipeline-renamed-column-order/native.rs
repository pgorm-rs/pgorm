//! Does a renamed column keep its declared position through distinct?
use pgorm::pgorm_query::{Alias, alias};
use pgorm::pipeline::{self as pl, ExprOps, Pipeline};

fn main() {
    let (sql, _) = Pipeline::from_schema(Alias::new("fixture"), Alias::new("accounts"))
        .select((
            pl::col(Alias::new("accounts"), Alias::new("id")),
            pl::col(Alias::new("accounts"), Alias::new("tenant")).as_(alias("p_tenant")),
            pl::col(Alias::new("accounts"), Alias::new("name")),
        ))
        .distinct()
        .into_sql()
        .expect("compiles");
    println!("{sql}");
}
