//! Does a renamed column keep its declared position through distinct?

use pgorm::pgorm_query::Name;
use pgorm::pgorm_query::alias;
use pgorm::pipeline::{self as pl, ExprOps, Pipeline};

fn main() {
    let (sql, _) = Pipeline::from_schema(Name::runtime("fixture"), Name::runtime("accounts"))
        .select((
            pl::col(Name::runtime("accounts"), Name::runtime("id")),
            pl::col(Name::runtime("accounts"), Name::runtime("tenant")).as_(alias("p_tenant")),
            pl::col(Name::runtime("accounts"), Name::runtime("name")),
        ))
        .distinct()
        .into_sql()
        .expect("compiles");
    println!("{sql}");
}
