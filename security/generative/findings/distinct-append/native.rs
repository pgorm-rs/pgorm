use pgorm::pgorm_query::Name;

use pgorm::pipeline::{self as pl, Pipeline};

fn main() {
    let projected = Pipeline::from_schema(Name::runtime("fixture"), Name::runtime("accounts"))
        .select(pl::col(Name::runtime("accounts"), Name::runtime("id")));
    println!("baseline: {:?}", projected.clone().append(projected.clone()).into_sql());
    let distinct = projected.distinct();
    println!("distinct: {:?}", distinct.clone().into_sql());
    let appended = distinct.clone().append(distinct).into_sql();
    println!("distinct then append: {appended:?}");
    assert!(appended.is_ok(), "matching explicit projections should remain composable");
}
