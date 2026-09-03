use pgorm_query::Alias;

use crate::tests_cfg::{cake, cake_filling_price};

use super::adapter::compile_text;
use super::*;

fn invoice() -> Alias {
    Alias::new("invoice")
}

fn customer() -> Alias {
    Alias::new("customer")
}

fn total() -> Alias {
    Alias::new("total")
}

fn customer_id() -> Alias {
    Alias::new("customer_id")
}

/// Golden output plus the pg_query oracle: the emitted SQL must be a string
/// the real PostgreSQL grammar accepts.
// [spec:pgorm:req:pipeline.errors/test]
fn sql_of(pipeline: Pipeline) -> String {
    let (sql, _) = pipeline.into_sql().expect("pipeline compiles");
    if let Err(err) = pg_query::parse(&sql) {
        panic!("PostgreSQL grammar rejected the emitted SQL: {err}\n  {sql}");
    }
    sql
}

// [spec:pgorm:def:pipeline.adapter+1/test]    direct PL construction is
// interchangeable with compiling the equivalent PRQL text
#[test]
fn built_filter_matches_text_compilation() {
    let built = sql_of(
        Pipeline::from(invoice()).filter(|binder| col(invoice(), total()).gt(binder.bind(5_i64))),
    );
    let text = compile_text("from invoice | filter invoice.total > $1").expect("compiles");
    assert_eq!(built, text);
}

// [spec:pgorm:sem:pipeline.qualify/test]
#[test]
fn schema_qualified_from_renders_both_parts() {
    let built = sql_of(Pipeline::from_schema(Alias::new("archive"), invoice()));
    assert_eq!(built, "SELECT * FROM archive.invoice");
}

// [spec:pgorm:sem:pipeline.qualify/test]
#[test]
fn quoted_identifiers_survive_rendering() {
    let built = sql_of(
        Pipeline::from(Alias::new("User Order"))
            .select(|_| vec![col(Alias::new("User Order"), Alias::new("Total Price"))]),
    );
    assert_eq!(built, r#"SELECT "Total Price" FROM "User Order""#);
}

// [spec:pgorm:sem:pipeline.qualify/test]
#[test]
fn from_entity_uses_table_metadata() {
    let built = sql_of(Pipeline::from_entity::<cake::Entity>());
    assert_eq!(built, "SELECT * FROM cake");
}

// [spec:pgorm:sem:pipeline.qualify/test]
#[test]
fn from_entity_honours_schema_name() {
    let built = sql_of(Pipeline::from_entity::<cake_filling_price::Entity>());
    assert_eq!(built, "SELECT * FROM public.cake_filling_price");
}

// [spec:pgorm:req:pipeline.surface/test]
#[test]
fn cast_renders_as_postgres_cast() {
    let built = sql_of(
        Pipeline::from(invoice())
            .derive(|_| vec![col(invoice(), total()).cast(CastType::Integer).aliased("t")]),
    );
    assert_eq!(built, "SELECT *, CAST(total AS integer) AS t FROM invoice");
}

// [spec:pgorm:req:pipeline.surface/test]
#[test]
fn in_array_of_bound_params_renders_in_list() {
    let built = sql_of(Pipeline::from(invoice()).filter(|binder| {
        col(invoice(), total()).in_array(vec![binder.bind(1_i64), binder.bind(2_i64)])
    }));
    assert_eq!(built, "SELECT * FROM invoice WHERE total IN ($1, $2)");
}

// [spec:pgorm:req:pipeline.surface/test]
#[test]
fn take_range_renders_limit_offset() {
    let built = sql_of(Pipeline::from(invoice()).take_range(21..=30));
    assert_eq!(built, "SELECT * FROM invoice LIMIT 10 OFFSET 20");
}

// [spec:pgorm:req:pipeline.surface/test]
#[test]
fn case_renders_case_when() {
    let built = sql_of(Pipeline::from(invoice()).derive(|_| {
        vec![
            case(
                vec![(col(invoice(), total()).gt(lit_int(100)), lit_str("big"))],
                lit_str("small"),
            )
            .aliased("bucket"),
        ]
    }));
    assert_eq!(
        built,
        "SELECT *, CASE WHEN total > 100 THEN 'big' ELSE 'small' END AS bucket FROM invoice"
    );
}

// [spec:pgorm:req:pipeline.surface/test]
#[test]
fn null_handling_renders_is_null_forms() {
    let built = sql_of(Pipeline::from(invoice()).filter(|_| {
        col(invoice(), total())
            .is_null()
            .or(col(invoice(), total()).is_not_null())
    }));
    assert_eq!(
        built,
        "SELECT * FROM invoice WHERE total IS NULL OR total IS NOT NULL"
    );
}

// [spec:pgorm:req:pipeline.surface/test]
#[test]
fn coalesce_and_arithmetic_render_inline() {
    let built = sql_of(Pipeline::from(invoice()).derive(|_| {
        vec![
            (col(invoice(), total()).coalesce(lit_float(0.0)) * lit_float(1.1)).aliased("gross"),
            (col(invoice(), total()) + lit_int(1) - lit_int(2)).aliased("adjusted"),
            (col(invoice(), total()) / lit_float(2.0)).aliased("half"),
            (col(invoice(), total()) % lit_int(10)).aliased("cents"),
        ]
    }));
    assert_eq!(
        built,
        "SELECT *, COALESCE(total, 0.0) * 1.1 AS gross, total + 1 - 2 AS adjusted, \
         (total * 1.0 / 2.0) AS half, total % 10 AS cents FROM invoice"
    );
}

// [spec:pgorm:req:pipeline.surface/test]
#[test]
fn aggregate_functions_render_expected_sql() {
    let built = sql_of(Pipeline::from(invoice()).aggregate_by(|_| {
        (
            vec![col(invoice(), customer_id())],
            vec![
                sum(col(invoice(), total())).aliased("s"),
                min(col(invoice(), total())).aliased("lo"),
                max(col(invoice(), total())).aliased("hi"),
                average(col(invoice(), total())).aliased("mean"),
                stddev(col(invoice(), total())).aliased("sd"),
                count_rows().aliased("n"),
                count_distinct(col(invoice(), total())).aliased("distinct_totals"),
            ],
        )
    }));
    assert_eq!(
        built,
        "SELECT customer_id, COALESCE(SUM(total), 0) AS s, MIN(total) AS lo, MAX(total) AS hi, \
         AVG(total) AS mean, STDDEV(total) AS sd, COUNT(*) AS n, \
         COUNT(DISTINCT total) AS distinct_totals FROM invoice GROUP BY customer_id"
    );
}

// [spec:pgorm:req:pipeline.surface/test]
#[test]
fn filter_after_aggregate_lands_in_having() {
    let built = sql_of(
        Pipeline::from(invoice())
            .aggregate_by(|_| {
                (
                    vec![col(invoice(), customer_id())],
                    vec![sum(col(invoice(), total())).aliased("total_spent")],
                )
            })
            .filter(|binder| out("total_spent").gt(binder.bind(40.0_f64)))
            .sort(|_| vec![out("total_spent").desc()])
            .take(5),
    );
    assert_eq!(
        built,
        "SELECT customer_id, COALESCE(SUM(total), 0) AS total_spent FROM invoice \
         GROUP BY customer_id HAVING COALESCE(SUM(total), 0) > $1 \
         ORDER BY total_spent DESC LIMIT 5"
    );
    let parsed = parsed_select(&built);
    assert!(parsed.having_clause.is_some());
}

// [spec:pgorm:req:pipeline.surface/test]
#[test]
fn filter_after_window_nests_through_cte() {
    let built = sql_of(
        Pipeline::from(invoice())
            .window(|_| {
                WindowDef::derive(vec![row_number().aliased("rn")])
                    .partition_by(vec![col(invoice(), customer_id())])
                    .sorted(vec![col(invoice(), total()).desc()])
            })
            .filter(|binder| out("rn").lte(binder.bind(2_i64))),
    );
    assert_eq!(
        built,
        "WITH table_0 AS (SELECT *, ROW_NUMBER() OVER (PARTITION BY customer_id \
         ORDER BY total DESC) AS rn FROM invoice) SELECT * FROM table_0 WHERE rn <= $1"
    );
    let parsed = parsed_select(&built);
    assert!(parsed.with_clause.is_some());
    assert!(parsed.where_clause.is_some());
}

// [spec:pgorm:req:pipeline.surface/test]
#[test]
fn window_frame_renders_rows_between() {
    let built = sql_of(Pipeline::from(invoice()).window(|_| {
        WindowDef::derive(vec![sum(col(invoice(), total())).aliased("running")])
            .sorted(vec![col(invoice(), Alias::new("id"))])
            .frame(Frame::rows(Some(-2), Some(0)))
    }));
    assert_eq!(
        built,
        "SELECT *, SUM(total) OVER (ORDER BY id ROWS BETWEEN 2 PRECEDING AND CURRENT ROW) AS running FROM invoice ORDER BY id"
    );
}

// [spec:pgorm:req:pipeline.surface/test]
#[test]
fn lag_lead_first_last_render_window_calls() {
    let built = sql_of(Pipeline::from(invoice()).window(|_| {
        WindowDef::derive(vec![
            lag(1, col(invoice(), total())).aliased("prev"),
            lead(1, col(invoice(), total())).aliased("next"),
            first(col(invoice(), total())).aliased("head"),
            last(col(invoice(), total())).aliased("tail"),
            rank(col(invoice(), total())).aliased("r"),
            rank_dense(col(invoice(), total())).aliased("rd"),
        ])
        .partition_by(vec![col(invoice(), customer_id())])
        .sorted(vec![col(invoice(), total())])
    }));
    assert_eq!(
        built,
        "SELECT *, LAG(total, 1) OVER (PARTITION BY customer_id ORDER BY total) AS prev, \
         LEAD(total, 1) OVER (PARTITION BY customer_id ORDER BY total) AS next, \
         FIRST_VALUE(total) OVER (PARTITION BY customer_id ORDER BY total) AS head, \
         LAST_VALUE(total) OVER (PARTITION BY customer_id ORDER BY total) AS tail, \
         RANK() OVER (PARTITION BY customer_id ORDER BY total) AS r, \
         DENSE_RANK() OVER (PARTITION BY customer_id ORDER BY total) AS rd FROM invoice"
    );
}

// [spec:pgorm:req:pipeline.surface/test]
#[test]
fn join_takes_an_explicit_condition() {
    let built = sql_of(
        Pipeline::from(invoice()).join(JoinSide::Left, customer(), |_| {
            col(invoice(), customer_id()).eq(col(customer(), Alias::new("id")))
        }),
    );
    assert_eq!(
        built,
        "SELECT invoice.*, customer.* FROM invoice \
         LEFT OUTER JOIN customer ON invoice.customer_id = customer.id"
    );
}

// [spec:pgorm:req:pipeline.params/test]
#[test]
fn placeholders_number_in_bind_order_across_stages() {
    let (sql, values) = Pipeline::from(invoice())
        .filter(|binder| col(invoice(), total()).gt(binder.bind(1_i64)))
        .derive(|binder| vec![(col(invoice(), total()) * binder.bind(2.0_f64)).aliased("gross")])
        .filter(|binder| out("gross").lt(binder.bind(3_i64)))
        .into_sql()
        .expect("pipeline compiles");
    assert_eq!(values.0.len(), 3);
    for placeholder in ["$1", "$2", "$3"] {
        assert!(sql.contains(placeholder), "{placeholder} missing: {sql}");
    }
}

// [spec:pgorm:req:pipeline.surface/test]
#[test]
fn scopes_compose_as_pipeline_functions() {
    fn expensive(pipeline: Pipeline) -> Pipeline {
        pipeline.filter(|binder| col(invoice(), total()).gt(binder.bind(100_i64)))
    }
    fn newest_first(pipeline: Pipeline) -> Pipeline {
        pipeline.sort(|_| vec![col(invoice(), Alias::new("invoice_date")).desc()])
    }
    let built = sql_of(newest_first(expensive(Pipeline::from(invoice()))).take(3));
    assert_eq!(
        built,
        "SELECT * FROM invoice WHERE total > $1 ORDER BY invoice_date DESC LIMIT 3"
    );
}

// [spec:pgorm:req:pipeline.errors/test]
#[test]
fn reserved_alias_is_a_typed_error() {
    let err = Pipeline::from(invoice())
        .derive(|_| vec![col(invoice(), total()).aliased("sum")])
        .into_sql()
        .expect_err("reserved alias must be refused");
    assert_eq!(err, PipelineError::ReservedAlias("sum".to_owned()));
}

// [spec:pgorm:req:pipeline.errors/test]
#[test]
fn stdlib_name_reference_is_a_compile_error() {
    let err = Pipeline::from(invoice())
        .filter(|_| out("count").gt(lit_int(1)))
        .into_sql()
        .expect_err("std name as a value must be refused");
    assert!(matches!(err, PipelineError::Compile(_)));
}

fn parsed_select(sql: &str) -> pg_query::protobuf::SelectStmt {
    let parsed = pg_query::parse(sql).expect("grammar accepts");
    let node = parsed.protobuf.stmts[0]
        .stmt
        .as_ref()
        .and_then(|stmt| stmt.node.as_ref())
        .expect("statement present");
    match node {
        pg_query::NodeEnum::SelectStmt(select) => (**select).clone(),
        other => panic!("expected SelectStmt, got {other:?}"),
    }
}

// [spec:pgorm:sem:pipeline.qualify/test]
#[test]
fn reserved_word_table_is_quoted() {
    let built = sql_of(Pipeline::from(Alias::new("order")).select(|_| {
        vec![
            col(Alias::new("order"), Alias::new("id")),
            col(Alias::new("order"), total()),
        ]
    }));
    assert_eq!(built, r#"SELECT id, total FROM "order""#);
}
