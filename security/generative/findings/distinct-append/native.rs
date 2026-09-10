use pgorm::pgorm_query::Alias;
use pgorm::pipeline::{self as pl, Pipeline};

fn main() {
    let projected = Pipeline::from_schema(Alias::new("fixture"), Alias::new("accounts"))
        .select(pl::col(Alias::new("accounts"), Alias::new("id")));
    println!("baseline: {:?}", projected.clone().append(projected.clone()).into_sql());
    let distinct = projected.distinct();
    println!("distinct: {:?}", distinct.clone().into_sql());
    let appended = distinct.clone().append(distinct).into_sql();
    println!("distinct then append: {appended:?}");
    assert!(appended.is_ok(), "matching explicit projections should remain composable");
}
