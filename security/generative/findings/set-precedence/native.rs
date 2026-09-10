use pgorm::pgorm_query::Alias;
use pgorm::pipeline::{self as pl, Pipeline};

fn main() {
    let original = Pipeline::from_schema(Alias::new("fixture"), Alias::new("accounts"))
        .select(pl::col(Alias::new("accounts"), Alias::new("id")));
    let appended = original.clone().append(original.clone());
    let intersected = appended.clone().intersect(original.clone());
    let queries = [appended, intersected.clone(), intersected.remove(original)];
    let sql: Vec<_> = queries
        .into_iter()
        .map(|query| {
            let (sql, values) = query.into_sql().expect("set program compiles");
            assert!(values.0.is_empty(), "this reproducer has no parameters");
            sql
        })
        .collect();
    println!("{}", serde_json::to_string(&sql).expect("SQL serializes"));
}
