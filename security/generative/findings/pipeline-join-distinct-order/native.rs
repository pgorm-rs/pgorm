//! Compile one joined, deduplicated pipeline repeatedly and report whether the
//! emitted projection order is stable.

use std::collections::BTreeSet;

use pgorm::pgorm_query::{Alias, alias};
use pgorm::pipeline::{self as pl, ExprOps, JoinSide, Pipeline};

fn compile() -> String {
    let inner = Pipeline::from_schema(Alias::new("fixture"), Alias::new("notes")).select((
        pl::col(Alias::new("notes"), Alias::new("id")).as_(alias("j_id")),
        pl::col(Alias::new("notes"), Alias::new("account_id")).as_(alias("j_account_id")),
    ));
    let (sql, _) = Pipeline::from_schema(Alias::new("fixture"), Alias::new("accounts"))
        .select((
            pl::col(Alias::new("accounts"), Alias::new("id")).as_(alias("p_id")),
            pl::col(Alias::new("accounts"), Alias::new("rank")).as_(alias("p_rank")),
        ))
        .join(
            JoinSide::Left,
            pl::named_runtime(inner, Alias::new("n")),
            pl::that(Alias::new("j_account_id")).eq(pl::this(Alias::new("p_rank"))),
        )
        .distinct()
        .into_sql()
        .expect("the joined, deduplicated pipeline compiles");
    sql
}

fn main() {
    let renderings: BTreeSet<String> = (0..40).map(|_| compile()).collect();
    println!("distinct renderings: {}", renderings.len());
    for sql in &renderings {
        let tail = sql.find("SELECT DISTINCT").map_or(sql.as_str(), |at| &sql[at..]);
        println!("  {}", &tail[..tail.len().min(110)]);
    }
    assert_eq!(
        renderings.len(),
        1,
        "the same pipeline must compile to one projection order"
    );
}
