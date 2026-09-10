use pgorm::pgorm_query::Alias;
use pgorm::pipeline::{self as pl, ExprOps, Pipeline};

fn main() {
    let relation = || Pipeline::from_schema(Alias::new("fixture"), Alias::new("accounts"));
    let score = || pl::col(Alias::new("accounts"), Alias::new("score"));
    let identity = pl::col(Alias::new("accounts"), Alias::new("id"));
    let (count, _) = relation()
        .window(pl::count(score()).as_("present"), pl::over())
        .into_sql()
        .expect("count compiles");
    let (frame, _) = relation()
        .window(
            (
                pl::first(score()).as_("head"),
                pl::last(score()).as_("tail"),
            ),
            pl::sort_by(identity).rows(Some(1), Some(1)),
        )
        .into_sql()
        .expect("window compiles");
    println!("count: {count}");
    println!("explicit frame: {frame}");
    let count_preserved = count.contains("COUNT(score)");
    let frames_preserved = frame
        .matches("ROWS BETWEEN 1 FOLLOWING AND 1 FOLLOWING")
        .count()
        == 2;
    println!("count argument preserved: {count_preserved}");
    println!("both explicit frames preserved: {frames_preserved}");
    assert!(
        count_preserved && frames_preserved,
        "count's argument and first/last's explicit frame must be preserved"
    );
}
