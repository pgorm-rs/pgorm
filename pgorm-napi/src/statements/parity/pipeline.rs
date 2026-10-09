//! The pipeline family's cases, built with pgorm::pipeline directly: every
//! value the JavaScript case binds, by a binder or as an operand, bound here
//! by the same stage's binder in the same order.

use pgorm::{
    pgorm_query::Values,
    pipeline::{self as pl, ExprOps, JoinSide, Pipeline, alias},
};

use super::n;

macro_rules! col {
    ($column:expr) => {
        pl::col(n("items"), n($column))
    };
}

macro_rules! base {
    () => {
        Pipeline::from(n("items"))
    };
}

macro_rules! left {
    () => {
        base!()
            .filter_with(|b| col!("id").gt(b.bind(1i32)))
            .select((col!("id"), col!("category"), col!("amount")))
    };
}

macro_rules! right {
    () => {
        base!()
            .filter_with(|b| col!("id").lt(b.bind(8i32)))
            .select((col!("id"), col!("category"), col!("amount")))
    };
}

fn sql(pipeline: Pipeline) -> (String, Values) {
    pipeline.into_sql().expect("the pipeline compiles")
}

fn bound() -> Pipeline {
    base!()
        .filter_with(|b| {
            let id = col!("id").gt(b.bind(2i64));
            id.and(col!("category").ne(b.bind("O'Brien; -- 雪")))
        })
        .select((col!("id"), col!("amount")))
}

fn repeated() -> Pipeline {
    base!().filter_with(|b| {
        b.bind("unused");
        let value = b.bind(2i32);
        col!("id")
            .gt(value.clone())
            .and(col!("id").lt(value.add(b.bind(10i64))))
    })
}

fn grouped() -> Pipeline {
    base!()
        .group(col!("category"))
        .aggregate((
            pl::sum(col!("amount")).as_(alias("total")),
            pl::count_rows().as_("n"),
        ))
        .filter_with(|b| alias("total").gt(b.bind(3i64)))
}

fn group_with() -> Pipeline {
    base!()
        .group_with(|b| [col!("category").coalesce(b.bind("missing"))])
        .aggregate_with(|b| {
            [pl::sum(col!("amount"))
                .add(b.bind(1i64))
                .as_(alias("total"))]
        })
}

fn window() -> Pipeline {
    let over = pl::by(col!("category"))
        .sort_by(col!("id"))
        .rows(None, Some(0));
    base!()
        .window(pl::row_number().as_(alias("row_rank")), over)
        .filter(alias("row_rank").lte(2i64))
}

fn window_functions() -> Pipeline {
    let over = pl::by(col!("category")).sort_by(col!("amount").desc());
    base!().window(
        vec![
            pl::rank(col!("amount")).as_("r"),
            pl::rank_dense(col!("amount")).as_("d"),
            pl::lag(1, col!("amount")).as_("before"),
            pl::lead(2, col!("amount")).as_("after"),
            pl::first(col!("amount")).as_("lowest"),
            pl::last(col!("amount")).as_("highest"),
        ],
        over,
    )
}

fn joins() -> [(&'static str, (String, Values)); 3] {
    [
        (
            "join",
            sql(base!()
                .join(
                    JoinSide::Left,
                    pl::named_runtime(n("items"), n("peer")),
                    col!("id").eq(pl::col(n("peer"), n("id"))),
                )
                .select((
                    col!("id"),
                    pl::col(n("peer"), n("amount")).as_("peer_amount"),
                ))),
        ),
        (
            "join-with",
            sql(base!().join_with(
                JoinSide::Inner,
                pl::named_runtime(right!(), n("peer")),
                |b| {
                    col!("id")
                        .eq(pl::col(n("peer"), n("id")))
                        .and(col!("amount").gt(b.bind(1i64)))
                },
            )),
        ),
        (
            "join-roles",
            sql(Pipeline::from(left!()).join(
                JoinSide::Full,
                right!(),
                pl::this(n("id")).eq(pl::that(n("id"))),
            )),
        ),
    ]
}

fn expressions() -> Pipeline {
    base!().derive_with(|b| {
        let size = pl::case(
            [
                (col!("amount").gt(b.bind(10i64)), pl::Expr::from("big")),
                (col!("amount").is_null(), pl::Expr::from("none")),
            ],
            b.bind("small"),
        );
        let arithmetic = -col!("amount")
            .sub(b.bind(1i64))
            .mul(b.bind(2i64))
            .div(b.bind(3i64))
            .rem(b.bind(4i64));
        let other = !col!("category").in_array([b.bind("a"), pl::Expr::from("b")]);
        let present = col!("id").is_not_null().or(col!("id").gte(b.bind(0i64)));
        [
            size.as_("size"),
            col!("amount").cast(pl::CastType::Text).as_("spelled"),
            arithmetic.as_("arith"),
            other.as_("other"),
            present.as_("present"),
            col!("amount").coalesce(pl::null()).as_("filled"),
        ]
    })
}

/// Every case, named as the golden file names it.
pub(super) fn cases() -> Vec<(&'static str, (String, Values))> {
    let mut cases = vec![
        (
            "literal",
            sql(base!()
                .filter(col!("id").gt(2i64))
                .select((col!("id"), col!("amount")))
                .sort(col!("id").desc())
                .take(3)),
        ),
        ("bound-operand", sql(bound())),
        (
            "literal-string",
            sql(base!().filter(col!("category").eq("O'Brien; -- \\ 雪"))),
        ),
        ("repeated", sql(repeated())),
        (
            "derive",
            sql(base!()
                .derive(col!("amount").add(2i64).as_(alias("total")))
                .filter_with(|b| alias("total").gt(b.bind(5i64)))),
        ),
        (
            "derive-with",
            sql(base!().derive_with(|b| [col!("amount").add(b.bind(2i64)).as_(alias("total"))])),
        ),
        (
            "select-with",
            sql(base!().select_with(|b| [col!("id"), b.bind("payload").as_("payload")])),
        ),
        ("group", sql(grouped())),
        ("group-with", sql(group_with())),
        ("window", sql(window())),
        (
            "window-with",
            sql(
                base!().window_with(pl::sort_by(col!("id")).range(Some(-1), Some(1)), |b| {
                    [pl::sum(col!("amount"))
                        .add(b.bind(2i64))
                        .as_(alias("total"))]
                }),
            ),
        ),
        ("window-functions", sql(window_functions())),
        (
            "sort-with",
            sql(base!()
                .sort_with(|b| [col!("id").add(b.bind(1i64)), col!("amount").desc()])
                .take_range(2..=4)),
        ),
        ("append", sql(left!().append(right!()))),
        ("intersect", sql(left!().intersect(right!()))),
        ("remove", sql(left!().remove(right!()))),
        (
            "embedded",
            sql(Pipeline::from(left!()).filter_with(|b| alias("id").gt(b.bind(5i64)))),
        ),
        ("distinct", sql(base!().select(col!("category")).distinct())),
        (
            "schema-source",
            sql(Pipeline::from_schema(n("app"), n("items")).join(
                JoinSide::Inner,
                pl::named_runtime(Pipeline::from_schema(n("app"), n("items")), n("other")),
                col!("id").eq(pl::col(n("other"), n("id"))),
            )),
        ),
        (
            "aggregates",
            sql(base!().group(col!("category")).aggregate(vec![
                pl::min(col!("amount")).as_("low"),
                pl::max(col!("amount")).as_("high"),
                pl::average(col!("amount")).as_("mean"),
                pl::stddev(col!("amount")).as_("spread"),
                pl::count(col!("amount")).as_("counted"),
                pl::count_distinct(col!("amount")).as_("kinds"),
            ])),
        ),
        ("expressions", sql(expressions())),
    ];
    cases.extend(joins());
    cases
}
