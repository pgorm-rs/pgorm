//! Compile one joined, deduplicated pipeline repeatedly and report whether the
//! emitted projection order is stable.

use pgorm::pgorm_query::Name;

use std::collections::BTreeSet;

use pgorm::pgorm_query::{alias};
use pgorm::pipeline::{self as pl, ExprOps, JoinSide, Pipeline};

fn compile() -> String {
    let inner = Pipeline::from_schema(Name::runtime("fixture"), Name::runtime("notes")).select((
        pl::col(Name::runtime("notes"), Name::runtime("id")).as_(alias("j_id")),
        pl::col(Name::runtime("notes"), Name::runtime("account_id")).as_(alias("j_account_id")),
    ));
    let (sql, _) = Pipeline::from_schema(Name::runtime("fixture"), Name::runtime("accounts"))
        .select((
            pl::col(Name::runtime("accounts"), Name::runtime("id")).as_(alias("p_id")),
            pl::col(Name::runtime("accounts"), Name::runtime("rank")).as_(alias("p_rank")),
        ))
        .join(
            JoinSide::Left,
            pl::named_runtime(inner, Name::runtime("n")),
            pl::that(Name::runtime("j_account_id")).eq(pl::this(Name::runtime("p_rank"))),
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
