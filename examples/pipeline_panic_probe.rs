//! Reproduces the SQL-generation panic recorded as `pipeline-native-panic`,
//! reduced from campaign item runtime-3287. Needs no database: the failure is
//! in compilation, before anything is sent.
//!
//! The shape is a relation sorted on six columns, a `select` projecting three
//! of them away, and then a `join` and a `distinct` forcing a new SELECT
//! scope. The retained sort still references the dropped columns, and ORDER BY
//! generation panics because those column ids no longer have names.
use pgorm::pgorm_query::Alias;
use pgorm::pipeline::{Expr, IntoSource, JoinSide, Pipeline, named_runtime};

fn main() {
    let name = |text| Expr::from(Alias::new(text));
    let deduplicated = Pipeline::from_schema(Alias::new("fixture"), Alias::new("accounts"))
        .sort([
            name("p_id"),
            name("p_tenant"),
            name("p_name"),
            name("p_score"),
            name("p_rank"),
            name("nonce"),
        ])
        .take_range(2i64..=4i64)
        .select([name("p_tenant"), name("p_name"), name("nonce")])
        .join(
            JoinSide::Inner,
            IntoSource::into_source(named_runtime(
                Pipeline::from_schema(Alias::new("fixture"), Alias::new("notes")),
                Alias::new("notes"),
            )),
            name("p_tenant"),
        )
        .distinct();

    println!("compiling…");
    match deduplicated.into_sql() {
        Ok((sql, values)) => println!("OK\n  sql: {sql}\n  values: {values:?}"),
        Err(error) => println!("ERROR (not a panic): {error}"),
    }
}
