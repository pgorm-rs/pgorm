use pgorm_query::{AliasName, Value, Values, alias};

use crate::tests_cfg::{cake, cake_filling_price, fruit};

use super::adapter::compile_text;
use super::*;

const INVOICE: AliasName = alias("invoice");
const CUSTOMER: AliasName = alias("customer");
const TOTAL: AliasName = alias("total");
const CUSTOMER_ID: AliasName = alias("customer_id");
const ID: AliasName = alias("id");

fn total<'brand>() -> Expr<'brand> {
    col(INVOICE, TOTAL)
}

/// Golden output plus the pg_query oracle: the emitted SQL must be a string
/// the real PostgreSQL grammar accepts.
// [spec:pgorm:req:pipeline.errors+1/test]
fn sql_of(pipeline: Pipeline) -> String {
    let (sql, _) = pipeline.into_sql().expect("pipeline compiles");
    if let Err(err) = pg_query::parse(&sql) {
        panic!("PostgreSQL grammar rejected the emitted SQL: {err}\n  {sql}");
    }
    sql
}

// [spec:pgorm:def:pipeline.adapter+2/test]    direct PL construction is
// interchangeable with compiling the equivalent PRQL text
#[test]
fn built_filter_matches_text_compilation() {
    let built =
        sql_of(Pipeline::from(INVOICE).filter_with(|binder| total().gt(binder.bind(5_i64))));
    let text = compile_text("from invoice | filter invoice.total > $1").expect("compiles");
    assert_eq!(built, text);
}

// [spec:pgorm:sem:pipeline.qualify+2/test]
#[test]
fn schema_qualified_from_renders_both_parts() {
    let built = sql_of(Pipeline::from_schema(alias("archive"), INVOICE));
    assert_eq!(built, "SELECT * FROM archive.invoice");
}

// [spec:pgorm:sem:pipeline.qualify+2/test]
#[test]
fn quoted_identifiers_survive_rendering() {
    let table = alias("User Order");
    let built = sql_of(Pipeline::from(table).select(col(table, alias("Total Price"))));
    assert_eq!(built, r#"SELECT "Total Price" FROM "User Order""#);
}

// [spec:pgorm:sem:pipeline.qualify+2/test]
#[test]
fn entity_source_uses_table_metadata() {
    let built = sql_of(Pipeline::from(cake::Entity));
    assert_eq!(built, "SELECT * FROM cake");
}

// [spec:pgorm:sem:pipeline.qualify+2/test]
#[test]
fn entity_source_honours_schema_name() {
    let built = sql_of(Pipeline::from(cake_filling_price::Entity));
    assert_eq!(built, "SELECT * FROM public.cake_filling_price");
}

// [spec:pgorm:sem:pipeline.qualify+2/test]    a column carries its own table
#[test]
fn entity_columns_are_qualified_by_construction() {
    let built = sql_of(
        Pipeline::from(cake::Entity)
            .join(
                JoinSide::Inner,
                fruit::Entity,
                cake::Column::Id.eq(fruit::Column::CakeId),
            )
            .select((cake::Column::Name, fruit::Column::Name)),
    );
    assert_eq!(
        built,
        "SELECT cake.name AS _expr_0, fruit.name FROM cake \
         INNER JOIN fruit ON cake.id = fruit.cake_id"
    );
}

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn cast_renders_as_postgres_cast() {
    let built = sql_of(Pipeline::from(INVOICE).derive(total().cast(CastType::Integer).as_("t")));
    assert_eq!(built, "SELECT *, CAST(total AS integer) AS t FROM invoice");
}

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn in_array_of_bound_params_renders_in_list() {
    let built = sql_of(
        Pipeline::from(INVOICE)
            .filter_with(|binder| total().in_array([binder.bind(1_i64), binder.bind(2_i64)])),
    );
    assert_eq!(built, "SELECT * FROM invoice WHERE total IN ($1, $2)");
}

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn in_array_of_literals_inlines_them() {
    let built = sql_of(Pipeline::from(INVOICE).filter(total().in_array([1, 2, 3])));
    assert_eq!(built, "SELECT * FROM invoice WHERE total IN (1, 2, 3)");
}

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn take_range_renders_limit_offset() {
    let built = sql_of(Pipeline::from(INVOICE).take_range(21..=30));
    assert_eq!(built, "SELECT * FROM invoice LIMIT 10 OFFSET 20");
}

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn case_renders_case_when() {
    let built = sql_of(
        Pipeline::from(INVOICE)
            .derive(case([(total().gt(100), "big")], "small").as_(alias("bucket"))),
    );
    assert_eq!(
        built,
        "SELECT *, CASE WHEN total > 100 THEN 'big' ELSE 'small' END AS bucket FROM invoice"
    );
}

// [spec:pgorm:req:pipeline.params+3/test]    a string literal is escaped, not
// interpolated: it cannot close the quote it is written into
#[test]
fn string_literals_are_escaped() {
    let built = sql_of(Pipeline::from(INVOICE).filter(col(INVOICE, alias("note")).eq("o'clock")));
    assert_eq!(built, "SELECT * FROM invoice WHERE note = 'o''clock'");
}

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn null_handling_renders_is_null_forms() {
    let built = sql_of(Pipeline::from(INVOICE).filter(total().is_null().or(total().is_not_null())));
    assert_eq!(
        built,
        "SELECT * FROM invoice WHERE total IS NULL OR total IS NOT NULL"
    );
}

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn coalesce_and_arithmetic_render_inline() {
    let built = sql_of(Pipeline::from(INVOICE).derive([
        (total().coalesce(0.0) * 1.1).as_(alias("gross")),
        (total() + 1 - 2).as_(alias("adjusted")),
        total().div(2.0).as_(alias("half")),
        total().rem(10).as_(alias("cents")),
    ]));
    assert_eq!(
        built,
        "SELECT *, COALESCE(total, 0.0) * 1.1 AS gross, total + 1 - 2 AS adjusted, \
         (total * 1.0 / 2.0) AS half, total % 10 AS cents FROM invoice"
    );
}

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn aggregate_functions_render_expected_sql() {
    let built = sql_of(
        Pipeline::from(INVOICE)
            .group(col(INVOICE, CUSTOMER_ID))
            .aggregate((
                sum(total()).as_("s"),
                min(total()).as_("lo"),
                max(total()).as_("hi"),
                average(total()).as_("mean"),
                stddev(total()).as_("sd"),
                count_rows().as_("n"),
                count_distinct(total()).as_("distinct_totals"),
            )),
    );
    assert_eq!(
        built,
        "SELECT customer_id, COALESCE(SUM(total), 0) AS s, MIN(total) AS lo, MAX(total) AS hi, \
         AVG(total) AS mean, STDDEV(total) AS sd, COUNT(*) AS n, \
         COUNT(DISTINCT total) AS distinct_totals FROM invoice GROUP BY customer_id"
    );
}

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn filter_after_aggregate_lands_in_having() {
    let spent = alias("total_spent");
    let built = sql_of(
        Pipeline::from(INVOICE)
            .group(col(INVOICE, CUSTOMER_ID))
            .aggregate(sum(total()).as_(spent))
            .filter_with(|binder| spent.gt(binder.bind(40.0_f64)))
            .sort(spent.desc())
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

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn filter_after_window_nests_through_cte() {
    let rn = alias("rn");
    let built = sql_of(
        Pipeline::from(INVOICE)
            .window(
                row_number().as_(rn),
                by(col(INVOICE, CUSTOMER_ID)).sort_by(total().desc()),
            )
            .filter_with(|binder| rn.lte(binder.bind(2_i64))),
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

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn window_frame_renders_rows_between() {
    let built = sql_of(Pipeline::from(INVOICE).window(
        sum(total()).as_(alias("running")),
        sort_by(col(INVOICE, ID)).rows(Some(-2), Some(0)),
    ));
    assert_eq!(
        built,
        "SELECT *, SUM(total) OVER (ORDER BY id ROWS BETWEEN 2 PRECEDING AND CURRENT ROW) AS running FROM invoice ORDER BY id"
    );
}

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn window_over_the_whole_relation_needs_no_keys() {
    let built = sql_of(Pipeline::from(INVOICE).window(sum(total()).as_(alias("grand")), over()));
    assert_eq!(built, "SELECT *, SUM(total) OVER () AS grand FROM invoice");
}

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn lag_lead_first_last_render_window_calls() {
    let built = sql_of(Pipeline::from(INVOICE).window(
        (
            lag(1, total()).as_("prev"),
            lead(1, total()).as_("next"),
            first(total()).as_("head"),
            last(total()).as_("tail"),
            rank(total()).as_("r"),
            rank_dense(total()).as_("rd"),
        ),
        by(col(INVOICE, CUSTOMER_ID)).sort_by(total()),
    ));
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

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn join_takes_an_explicit_condition() {
    let built = sql_of(Pipeline::from(INVOICE).join(
        JoinSide::Left,
        CUSTOMER,
        col(INVOICE, CUSTOMER_ID).eq(col(CUSTOMER, ID)),
    ));
    assert_eq!(
        built,
        "SELECT invoice.*, customer.* FROM invoice \
         LEFT OUTER JOIN customer ON invoice.customer_id = customer.id"
    );
}

// [spec:pgorm:req:pipeline.params+3/test]
#[test]
fn placeholders_number_in_bind_order_across_stages() {
    let gross = alias("gross");
    let (sql, values) = Pipeline::from(INVOICE)
        .filter_with(|binder| total().gt(binder.bind(1_i64)))
        .derive_with(|binder| [(total() * binder.bind(2.0_f64)).as_(gross)])
        .filter_with(|binder| gross.lt(binder.bind(3_i64)))
        .into_sql()
        .expect("pipeline compiles");
    assert_eq!(values.0.len(), 3);
    for placeholder in ["$1", "$2", "$3"] {
        assert!(sql.contains(placeholder), "{placeholder} missing: {sql}");
    }
}

// [spec:pgorm:req:pipeline.params+3/test]    a literal is inlined, a bound
// value is not
#[test]
fn literals_inline_and_bound_values_do_not() {
    let (sql, values) = Pipeline::from(INVOICE)
        .filter(total().gt(10))
        .filter_with(|binder| col(INVOICE, CUSTOMER_ID).eq(binder.bind(7_i32)))
        .into_sql()
        .expect("pipeline compiles");
    assert_eq!(
        sql,
        "SELECT * FROM invoice WHERE total > 10 AND customer_id = $1"
    );
    assert_eq!(values.0.len(), 1);
}

// [spec:pgorm:req:pipeline.surface+3/test]    one, a list, or a mix
#[test]
fn expression_lists_take_every_shape() {
    let one = sql_of(Pipeline::from(cake::Entity).select(cake::Column::Id));
    assert_eq!(one, "SELECT id FROM cake");

    let array = sql_of(Pipeline::from(cake::Entity).select([cake::Column::Id, cake::Column::Name]));
    assert_eq!(array, "SELECT id, name FROM cake");

    let owned = sql_of(Pipeline::from(cake::Entity).select(vec![cake::Column::Name]));
    assert_eq!(owned, "SELECT name FROM cake");

    let n = alias("n");
    let mixed = sql_of(
        Pipeline::from(cake::Entity)
            .derive(cake::Column::Id.add(1).as_(n))
            .select((cake::Column::Name, n, cake::Column::Id.mul(2))),
    );
    assert_eq!(mixed, "SELECT name, id + 1 AS n, id * 2 FROM cake");
}

// [spec:pgorm:req:pipeline.surface+3/test]
#[test]
fn scopes_compose_as_pipeline_functions() {
    fn expensive(pipeline: Pipeline) -> Pipeline {
        pipeline.filter_with(|binder| total().gt(binder.bind(100_i64)))
    }
    fn newest_first(pipeline: Pipeline) -> Pipeline {
        pipeline.sort(col(INVOICE, alias("invoice_date")).desc())
    }
    let built = sql_of(newest_first(expensive(Pipeline::from(INVOICE))).take(3));
    assert_eq!(
        built,
        "SELECT * FROM invoice WHERE total > $1 ORDER BY invoice_date DESC LIMIT 3"
    );
}

// [spec:pgorm:req:pipeline.errors+1/test]
#[test]
fn reserved_alias_is_a_typed_error() {
    let err = Pipeline::from(INVOICE)
        .derive(total().as_("sum"))
        .into_sql()
        .expect_err("reserved alias must be refused");
    assert_eq!(err, PipelineError::ReservedAlias("sum".to_owned()));
}

// [spec:pgorm:req:pipeline.errors+1/test]
#[test]
fn stdlib_name_reference_is_a_compile_error() {
    let err = Pipeline::from(INVOICE)
        .filter(alias("count").gt(1))
        .into_sql()
        .expect_err("std name as a value must be refused");
    assert!(matches!(err, PipelineError::Compile(_)));
}

// [spec:pgorm:req:pipeline.errors+1/test]    an unattached token is not a
// compile-time error; the server answers for it
#[test]
fn unattached_alias_token_compiles_to_a_column_reference() {
    let built = sql_of(Pipeline::from(INVOICE).filter(alias("never_declared").gt(1)));
    assert_eq!(built, "SELECT * FROM invoice WHERE never_declared > 1");
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

// [spec:pgorm:sem:pipeline.qualify+2/test]
#[test]
fn reserved_word_table_is_quoted() {
    let order = alias("order");
    let built = sql_of(Pipeline::from(order).select((col(order, ID), col(order, TOTAL))));
    assert_eq!(built, r#"SELECT id, total FROM "order""#);
}

// [spec:pgorm:req:pipeline.compose/test]    a pipeline is a from-source; its
// params keep their positions and the consumer's continue after them
#[test]
fn from_pipeline_binds_as_cte() {
    let spent = alias("spent");
    let (sql, values) = Pipeline::from(
        Pipeline::from(INVOICE)
            .group(col(INVOICE, CUSTOMER_ID))
            .aggregate(sum(total()).as_(spent))
            .filter_with(|binder| spent.gt(binder.bind(10_i64))),
    )
    .filter_with(|binder| CUSTOMER_ID.ne(binder.bind(7_i32)))
    .sort(spent.desc())
    .into_sql()
    .expect("pipeline compiles");
    pg_query::parse(&sql).expect("grammar accepts");
    assert_eq!(
        sql,
        "WITH table_0 AS (SELECT customer_id, COALESCE(SUM(total), 0) AS spent FROM invoice \
         GROUP BY customer_id HAVING COALESCE(SUM(total), 0) > $1) \
         SELECT customer_id, spent FROM table_0 WHERE customer_id <> $2 ORDER BY spent DESC"
    );
    assert_eq!(values.0.len(), 2);
}

// [spec:pgorm:req:pipeline.compose/test]    joining an aggregated pipeline:
// the consumer binds first, the embedded params renumber after it
#[test]
fn join_pipeline_renumbers_embedded_params() {
    let spent = alias("spent");
    let spenders = Pipeline::from(INVOICE)
        .group(col(INVOICE, CUSTOMER_ID))
        .aggregate(sum(total()).as_(spent))
        .filter_with(|binder| spent.gt(binder.bind(100_i64)));
    let (sql, values) = Pipeline::from(CUSTOMER)
        .filter_with(|binder| col(CUSTOMER, alias("active")).eq(binder.bind(true)))
        .join(JoinSide::Inner, spenders, col(CUSTOMER, ID).eq(CUSTOMER_ID))
        .select((col(CUSTOMER, alias("name")), spent))
        .into_sql()
        .expect("pipeline compiles");
    pg_query::parse(&sql).expect("grammar accepts");
    assert_eq!(
        sql,
        "WITH table_1 AS (SELECT name, id FROM customer WHERE active = $1), \
         table_0 AS (SELECT customer_id, COALESCE(SUM(total), 0) AS spent FROM invoice \
         GROUP BY customer_id HAVING COALESCE(SUM(total), 0) > $2) \
         SELECT table_1.name, table_0.spent FROM table_1 \
         INNER JOIN table_0 ON table_1.id = table_0.customer_id"
    );
    assert_eq!(values.0.len(), 2);
}

// [spec:pgorm:req:pipeline.compose/test]
#[test]
fn append_renders_union_all() {
    let (sql, values) = Pipeline::from(INVOICE)
        .filter_with(|binder| total().gt(binder.bind(1_i64)))
        .append(
            Pipeline::from(alias("archived_invoice"))
                .filter_with(|binder| col(alias("archived_invoice"), TOTAL).gt(binder.bind(2_i64))),
        )
        .into_sql()
        .expect("pipeline compiles");
    pg_query::parse(&sql).expect("grammar accepts");
    assert_eq!(
        sql,
        "SELECT * FROM invoice WHERE total > $1 \
         UNION ALL SELECT * FROM archived_invoice WHERE total > $2"
    );
    assert_eq!(values.0.len(), 2);
}

// [spec:pgorm:req:pipeline.compose/test]    distinct directly after append
// folds to UNION DISTINCT
#[test]
fn append_then_distinct_renders_union_distinct() {
    let built = sql_of(
        Pipeline::from(INVOICE)
            .select(col(INVOICE, CUSTOMER_ID))
            .append(Pipeline::from(alias("archive")).select(col(alias("archive"), CUSTOMER_ID)))
            .distinct(),
    );
    assert_eq!(
        built,
        "SELECT customer_id FROM invoice UNION DISTINCT SELECT customer_id FROM archive"
    );
}

// [spec:pgorm:req:pipeline.compose/test]
#[test]
fn intersect_renders_intersect_all() {
    let built = sql_of(
        Pipeline::from(INVOICE)
            .select(col(INVOICE, CUSTOMER_ID))
            .intersect(Pipeline::from(alias("refund")).select(col(alias("refund"), CUSTOMER_ID))),
    );
    assert_eq!(
        built,
        "WITH table_0 AS (SELECT customer_id FROM refund) \
         SELECT customer_id FROM invoice INTERSECT ALL SELECT * FROM table_0 AS b"
    );
}

// [spec:pgorm:req:pipeline.compose/test]
#[test]
fn remove_renders_except_all() {
    let built = sql_of(
        Pipeline::from(INVOICE)
            .select(col(INVOICE, CUSTOMER_ID))
            .remove(Pipeline::from(alias("refund")).select(col(alias("refund"), CUSTOMER_ID))),
    );
    assert_eq!(
        built,
        "WITH table_0 AS (SELECT customer_id FROM refund) \
         SELECT customer_id FROM invoice EXCEPT ALL SELECT * FROM table_0 AS b"
    );
}

// [spec:pgorm:req:pipeline.compose/test]
#[test]
fn distinct_alone_renders_select_distinct() {
    let built = sql_of(Pipeline::from(INVOICE).distinct());
    assert_eq!(built, "SELECT DISTINCT * FROM invoice");
}

// [spec:pgorm:req:pipeline.compose/test]    a set operation takes a plain
// table too
#[test]
fn append_accepts_a_table_source() {
    let built = sql_of(Pipeline::from(INVOICE).append(alias("archived_invoice")));
    assert_eq!(
        built,
        "SELECT * FROM invoice UNION ALL SELECT * FROM archived_invoice"
    );
}

// [spec:pgorm:req:pipeline.compose/test]    sort and take inside an embedded
// pipeline stay inside its CTE; PRQL's sticky sort carries outward
#[test]
fn take_and_sort_survive_embedding() {
    let (sql, values) = Pipeline::from(
        Pipeline::from(INVOICE)
            .filter_with(|binder| total().gt(binder.bind(5_i64)))
            .sort(total().desc())
            .take(3),
    )
    .filter(CUSTOMER_ID.gt(1))
    .into_sql()
    .expect("pipeline compiles");
    pg_query::parse(&sql).expect("grammar accepts");
    assert_eq!(
        sql,
        "WITH table_0 AS (SELECT * FROM invoice WHERE total > $1 ORDER BY total DESC LIMIT 3) \
         SELECT * FROM table_0 WHERE customer_id > 1 ORDER BY total DESC"
    );
    assert_eq!(values.0.len(), 1);
}

// [spec:pgorm:req:pipeline.compose/test]    embedding an embedder: bindings
// renumber past the consumer's and three params stay aligned
#[test]
fn nested_embedding_renumbers_bindings_and_params() {
    let inner = Pipeline::from(INVOICE).filter_with(|binder| total().gt(binder.bind(1_i64)));
    let middle = Pipeline::from(inner).filter_with(|binder| CUSTOMER_ID.gt(binder.bind(2_i64)));
    let (sql, values) = Pipeline::from(CUSTOMER)
        .filter_with(|binder| col(CUSTOMER, ID).gt(binder.bind(3_i64)))
        .join(
            JoinSide::Inner,
            middle,
            col(CUSTOMER, ID).eq(that(CUSTOMER_ID)),
        )
        .into_sql()
        .expect("pipeline compiles");
    pg_query::parse(&sql).expect("grammar accepts");
    assert_eq!(
        sql,
        "WITH table_2 AS (SELECT * FROM customer WHERE id > $1), \
         table_0 AS (SELECT * FROM invoice WHERE total > $2), \
         table_1 AS (SELECT * FROM table_0 WHERE customer_id > $3) \
         SELECT table_2.*, table_1.* FROM table_2 \
         INNER JOIN table_1 ON table_2.id = table_1.customer_id"
    );
    assert_eq!(values.0.len(), 3);
}

// [spec:pgorm:req:pipeline.compose/test]    the same alias declared in two
// composed pipelines lives in two scopes; neither collides
#[test]
fn duplicate_aliases_across_pipelines_coexist() {
    let n = alias("n");
    let other = Pipeline::from(alias("b")).derive((col(alias("b"), alias("x")) + 1).as_(n));
    let built = sql_of(
        Pipeline::from(alias("a"))
            .derive((col(alias("a"), alias("y")) + 2).as_(n))
            .join(JoinSide::Left, other, col(alias("a"), ID).eq(that(ID))),
    );
    assert_eq!(
        built,
        "WITH table_1 AS (SELECT *, y + 2 AS n FROM a), \
         table_0 AS (SELECT *, x + 1 AS n FROM b) \
         SELECT table_1.*, table_0.* FROM table_1 \
         LEFT OUTER JOIN table_0 ON table_1.id = table_0.id"
    );
}

// [spec:pgorm:req:pipeline.compose/test]    an unqualified name both sides
// export is refused by prqlc, by name
#[test]
fn ambiguous_embedded_column_is_a_compile_error() {
    let x = alias("x");
    let left = Pipeline::from(alias("a")).select(col(alias("a"), x));
    let right = Pipeline::from(alias("b")).select(col(alias("b"), x));
    let err = Pipeline::from(left)
        .join(JoinSide::Inner, right, x.eq(x))
        .into_sql()
        .expect_err("ambiguity must be refused");
    assert!(matches!(err, PipelineError::Compile(ref text) if text.contains("Ambiguous")));
}

// [spec:pgorm:req:pipeline.compose/test]    this() and that() name the two
// sides of the join when neither relation has a writable name
#[test]
fn that_qualifies_the_joined_relation() {
    let x = alias("x");
    let left = Pipeline::from(alias("a")).select(col(alias("a"), x));
    let right = Pipeline::from(alias("b")).select(col(alias("b"), x));
    let built = sql_of(Pipeline::from(left).join(JoinSide::Inner, right, this(x).eq(that(x))));
    assert_eq!(
        built,
        "WITH table_0 AS (SELECT x FROM a), table_1 AS (SELECT x FROM b) \
         SELECT table_0.x, table_1.x FROM table_0 \
         INNER JOIN table_1 ON table_0.x = table_1.x"
    );
}

// [spec:pgorm:req:pipeline.compose/test]    mismatched column counts are
// refused at compile time when prqlc can see both projections
#[test]
fn append_column_count_mismatch_is_refused() {
    let err = Pipeline::from(alias("a"))
        .select(col(alias("a"), alias("x")))
        .append(
            Pipeline::from(alias("b")).select((col(alias("b"), alias("y")), col(alias("b"), ID))),
        )
        .into_sql()
        .expect_err("column count mismatch must be refused");
    assert!(matches!(err, PipelineError::Compile(_)));
}

// [spec:pgorm:req:pipeline.compose/test]    a reserved alias inside an
// embedded pipeline is still screened
#[test]
fn reserved_alias_inside_embedded_pipeline_is_refused() {
    let err = Pipeline::from(Pipeline::from(INVOICE).derive(total().as_("sum")))
        .into_sql()
        .expect_err("reserved alias must be refused");
    assert_eq!(err, PipelineError::ReservedAlias("sum".to_owned()));
}

// [spec:pgorm:def:pipeline.adapter+2/test]    a let-bound composition built
// directly matches the same query compiled from PRQL text
#[test]
fn built_composition_matches_text_compilation() {
    let built = sql_of(
        Pipeline::from(
            Pipeline::from(INVOICE).filter_with(|binder| total().gt(binder.bind(5_i64))),
        )
        .filter(CUSTOMER_ID.gt(1)),
    );
    let text = compile_text(
        "let table_0 = (from invoice | filter invoice.total > $1)\n\
         from table_0 | filter customer_id > 1",
    )
    .expect("compiles");
    assert_eq!(built, text);
}

// [spec:pgorm:req:pipeline.compose/test]    after remove or intersect the
// relation is renamed, so later stages refer to columns by bare name
#[test]
fn stages_after_remove_use_bare_names() {
    let refund = Pipeline::from(alias("refund")).select(col(alias("refund"), CUSTOMER_ID));
    let built = sql_of(
        Pipeline::from(INVOICE)
            .select(col(INVOICE, CUSTOMER_ID))
            .remove(refund.clone())
            .sort(CUSTOMER_ID),
    );
    assert_eq!(
        built,
        "WITH table_0 AS (SELECT customer_id FROM refund), \
         table_1 AS (SELECT customer_id FROM invoice \
         EXCEPT ALL SELECT * FROM table_0 AS b) \
         SELECT customer_id FROM table_1 ORDER BY customer_id"
    );

    let err = Pipeline::from(INVOICE)
        .select(col(INVOICE, CUSTOMER_ID))
        .remove(refund)
        .sort(col(INVOICE, CUSTOMER_ID))
        .into_sql()
        .expect_err("the source qualification is gone after a set op");
    assert!(matches!(err, PipelineError::Compile(_)));
}

const EMPLOYEE: AliasName = alias("employee");
const MANAGER: AliasName = alias("manager");
const NAME: AliasName = alias("name");
const MANAGER_ID: AliasName = alias("manager_id");

// [spec:pgorm:sem:pipeline.self-join/test]    the classic employee-manager
// query: one table, two names, both sides selected
#[test]
fn a_named_operand_joins_a_table_to_itself() {
    let built = sql_of(
        Pipeline::from(EMPLOYEE)
            .join(
                JoinSide::Inner,
                EMPLOYEE.named(MANAGER),
                col(EMPLOYEE, MANAGER_ID).eq(col(MANAGER, ID)),
            )
            .select((col(EMPLOYEE, NAME), col(MANAGER, NAME).as_(alias("boss"))))
            .sort(col(EMPLOYEE, NAME)),
    );
    assert_eq!(
        built,
        "SELECT employee.name, manager.name AS boss FROM employee \
         INNER JOIN employee AS manager ON employee.manager_id = manager.id \
         ORDER BY employee.name"
    );
}

// [spec:pgorm:sem:pipeline.self-join/test]    the name reaches every stage
// after the join, not only the condition
#[test]
fn a_source_name_qualifies_later_stages() {
    let built = sql_of(
        Pipeline::from(EMPLOYEE)
            .join(
                JoinSide::Left,
                EMPLOYEE.named(MANAGER),
                col(EMPLOYEE, MANAGER_ID).eq(col(MANAGER, ID)),
            )
            .filter(col(MANAGER, NAME).is_not_null())
            .sort(col(MANAGER, NAME).desc()),
    );
    assert_eq!(
        built,
        "SELECT employee.*, manager.* FROM employee \
         LEFT OUTER JOIN employee AS manager ON employee.manager_id = manager.id \
         WHERE manager.name IS NOT NULL ORDER BY manager.name DESC"
    );
}

// [spec:pgorm:sem:pipeline.self-join/test]    naming a source replaces the
// name it had, as SQL's AS does
#[test]
fn a_named_source_drops_its_own_name() {
    let err = Pipeline::from(EMPLOYEE.named(alias("e")))
        .select(col(EMPLOYEE, NAME))
        .into_sql()
        .expect_err("the table name is gone once the source is named");
    assert!(matches!(err, PipelineError::Compile(ref text) if text.contains("Unknown name")));
}

// [spec:pgorm:sem:pipeline.self-join/test]    an entity is named the same
// way, and its columns then travel through col
#[test]
fn an_entity_operand_takes_a_name() {
    let peer = alias("peer");
    let built = sql_of(
        Pipeline::from(fruit::Entity)
            .join(
                JoinSide::Inner,
                fruit::Entity.named(peer),
                fruit::Column::CakeId.eq(col(peer, alias("cake_id"))),
            )
            .filter(fruit::Column::Id.lt(col(peer, ID)))
            .select((fruit::Column::Name, col(peer, NAME))),
    );
    assert_eq!(
        built,
        "SELECT fruit.name AS _expr_0, peer.name FROM fruit \
         INNER JOIN fruit AS peer ON fruit.cake_id = peer.cake_id \
         WHERE fruit.id < peer.id"
    );
}

// [spec:pgorm:sem:pipeline.self-join/test]    a named pipeline is a self-join
// over a derived relation, params and all
#[test]
fn a_named_pipeline_joins_as_an_aliased_cte() {
    let seniors = Pipeline::from(EMPLOYEE)
        .filter_with(|binder| col(EMPLOYEE, alias("level")).gt(binder.bind(3_i32)));
    let (sql, values) = Pipeline::from(EMPLOYEE)
        .join(
            JoinSide::Inner,
            seniors.named(MANAGER),
            col(EMPLOYEE, MANAGER_ID).eq(col(MANAGER, ID)),
        )
        .select((col(EMPLOYEE, NAME), col(MANAGER, NAME).as_(alias("boss"))))
        .into_sql()
        .expect("pipeline compiles");
    pg_query::parse(&sql).expect("grammar accepts");
    assert_eq!(
        sql,
        "WITH table_0 AS (SELECT * FROM employee WHERE level > $1) \
         SELECT employee.name, manager.name AS boss FROM employee \
         INNER JOIN table_0 AS manager ON employee.manager_id = manager.id"
    );
    assert_eq!(values.0.len(), 1);
}

// [spec:pgorm:sem:pipeline.self-join/test]    renaming inside an embedded
// pipeline reaches both sides without naming the operand
#[test]
fn renaming_before_embedding_reaches_both_sides() {
    let manager_pk = alias("manager_pk");
    let boss = alias("boss");
    let managers = Pipeline::from(EMPLOYEE).select((
        col(EMPLOYEE, ID).as_(manager_pk),
        col(EMPLOYEE, NAME).as_(boss),
    ));
    let built = sql_of(
        Pipeline::from(EMPLOYEE)
            .join(
                JoinSide::Inner,
                managers,
                col(EMPLOYEE, MANAGER_ID).eq(manager_pk),
            )
            .select((col(EMPLOYEE, NAME), boss)),
    );
    assert_eq!(
        built,
        "WITH table_0 AS (SELECT id AS manager_pk, name AS boss FROM employee) \
         SELECT employee.name, table_0.boss FROM employee \
         INNER JOIN table_0 ON employee.manager_id = table_0.manager_pk"
    );
}

// [spec:pgorm:sem:pipeline.self-join/test]    a chain of names: three
// generations of the same table in one query
#[test]
fn named_operands_chain_across_generations() {
    let grandparent = alias("grandparent");
    let built = sql_of(
        Pipeline::from(EMPLOYEE)
            .join(
                JoinSide::Inner,
                EMPLOYEE.named(MANAGER),
                col(EMPLOYEE, MANAGER_ID).eq(col(MANAGER, ID)),
            )
            .join(
                JoinSide::Inner,
                EMPLOYEE.named(grandparent),
                col(MANAGER, MANAGER_ID).eq(col(grandparent, ID)),
            )
            .select((
                col(EMPLOYEE, NAME),
                col(MANAGER, NAME).as_(alias("boss")),
                col(grandparent, NAME).as_(alias("skip")),
            )),
    );
    assert_eq!(
        built,
        "SELECT employee.name, manager.name AS boss, grandparent.name AS skip \
         FROM employee INNER JOIN employee AS manager ON employee.manager_id = manager.id \
         INNER JOIN employee AS grandparent ON manager.manager_id = grandparent.id"
    );
}

// [spec:pgorm:sem:pipeline.self-join/test]    a name is screened like any
// other introduced name
#[test]
fn a_reserved_source_name_is_refused() {
    let err = Pipeline::from(EMPLOYEE)
        .join(
            JoinSide::Inner,
            EMPLOYEE.named(alias("sum")),
            col(EMPLOYEE, MANAGER_ID).eq(col(alias("sum"), ID)),
        )
        .into_sql()
        .expect_err("a reserved name must be refused");
    assert_eq!(err, PipelineError::ReservedAlias("sum".to_owned()));
}

/// Like [`sql_of`], keeping the values: the emitted SQL must pass the
/// grammar oracle after any census rewrite too.
// [spec:pgorm:req:pipeline.params+3/test]
fn sql_and_values_of(pipeline: Pipeline) -> (String, Values) {
    let (sql, values) = pipeline.into_sql().expect("pipeline compiles");
    if let Err(err) = pg_query::parse(&sql) {
        panic!("PostgreSQL grammar rejected the emitted SQL: {err}\n  {sql}");
    }
    (sql, values)
}

fn ints(values: &Values) -> Vec<Value> {
    values.0.clone()
}

/// Three bound derivations to prune from: `a = $1`, `b = $2`, `c = $3`.
fn three_bound() -> Pipeline {
    Pipeline::from(cake::Entity).derive_with(|binder| {
        [
            binder.bind(1_i32).as_(alias("a")),
            binder.bind(2_i32).as_(alias("b")),
            binder.bind(3_i32).as_(alias("c")),
        ]
    })
}

// [spec:pgorm:req:pipeline.params+3/test]    a bound derivation nothing
// reads is pruned by the optimizer; its value must not survive it
#[test]
fn pruned_binding_drops_its_value() {
    let (sql, values) = sql_and_values_of(
        Pipeline::from(cake::Entity)
            .derive_with(|binder| [binder.bind(42_i32).as_(alias("unused"))])
            .select(cake::Column::Id),
    );
    assert_eq!(sql, "SELECT id FROM cake");
    assert!(values.0.is_empty(), "{values:?}");
}

// [spec:pgorm:req:pipeline.params+3/test]    pruning the first placeholder
// renumbers the survivors down
#[test]
fn pruning_the_first_placeholder_renumbers_survivors() {
    let (sql, values) = sql_and_values_of(three_bound().select((alias("b"), alias("c"))));
    assert_eq!(sql, "SELECT $1 AS b, $2 AS c FROM cake");
    assert_eq!(ints(&values), vec![2_i32.into(), 3_i32.into()]);
}

// [spec:pgorm:req:pipeline.params+3/test]    pruning a middle placeholder
// leaves a gap the census closes
#[test]
fn pruning_a_middle_placeholder_renumbers_survivors() {
    let (sql, values) = sql_and_values_of(three_bound().select((alias("a"), alias("c"))));
    assert_eq!(sql, "SELECT $1 AS a, $2 AS c FROM cake");
    assert_eq!(ints(&values), vec![1_i32.into(), 3_i32.into()]);
}

// [spec:pgorm:req:pipeline.params+3/test]    pruning the last placeholder
// changes no numbering but still drops the value
#[test]
fn pruning_the_last_placeholder_compacts_the_values() {
    let (sql, values) = sql_and_values_of(three_bound().select((alias("a"), alias("b"))));
    assert_eq!(sql, "SELECT $1 AS a, $2 AS b FROM cake");
    assert_eq!(ints(&values), vec![1_i32.into(), 2_i32.into()]);
}

// [spec:pgorm:req:pipeline.params+3/test]    pruning every placeholder
// leaves an unparameterized statement and no values at all
#[test]
fn pruning_every_placeholder_empties_the_values() {
    let (sql, values) = sql_and_values_of(three_bound().select(cake::Column::Id));
    assert_eq!(sql, "SELECT id FROM cake");
    assert!(values.0.is_empty(), "{values:?}");
}

// [spec:pgorm:req:pipeline.params+3/test]    a placeholder written twice
// keeps its value once, both occurrences renumbered alike
#[test]
fn repeated_placeholder_keeps_one_value() {
    let (sql, values) = sql_and_values_of(
        Pipeline::from(cake::Entity)
            .derive_with(|binder| [binder.bind(9_i32).as_(alias("unused"))])
            .filter_with(|binder| {
                let bound = binder.bind(7_i32);
                bound.clone().gt(0).and(bound.lt(100))
            })
            .select(cake::Column::Id),
    );
    assert_eq!(
        sql,
        "WITH table_0 AS (SELECT id FROM cake) \
         SELECT id FROM table_0 WHERE $1 > 0 AND $1 < 100"
    );
    assert_eq!(ints(&values), vec![7_i32.into()]);
}

// [spec:pgorm:req:pipeline.params+3/test]    an embedded pipeline's pruned
// binding sits below the consumer's surviving one, which renumbers down
// past it — the rebase offset and the census compose
#[test]
fn pruned_embedded_binding_renumbers_the_consumer() {
    let inner = Pipeline::from(fruit::Entity)
        .derive_with(|binder| [binder.bind(5_i32).as_(alias("unused"))])
        .select(fruit::Column::CakeId);
    let (sql, values) = sql_and_values_of(
        Pipeline::from(cake::Entity)
            .join(
                JoinSide::Inner,
                inner,
                cake::Column::Id.eq(alias("cake_id")),
            )
            .filter_with(|binder| cake::Column::Id.gt(binder.bind(3_i32)))
            .select(cake::Column::Name),
    );
    assert_eq!(
        sql,
        "WITH table_0 AS (SELECT cake_id FROM fruit) \
         SELECT cake.name FROM cake \
         INNER JOIN table_0 ON cake.id = table_0.cake_id \
         WHERE cake.id > $1"
    );
    assert_eq!(ints(&values), vec![3_i32.into()]);
}

// [spec:pgorm:req:pipeline.params+3/test]    the mirror ordering: the
// consumer's binding survives at $1 and the embedded pipeline's, rebased
// past it, is the one pruned
#[test]
fn pruned_embedded_binding_after_a_surviving_one() {
    let inner = Pipeline::from(fruit::Entity)
        .derive_with(|binder| [binder.bind(5_i32).as_(alias("unused"))])
        .select(fruit::Column::CakeId);
    let (sql, values) = sql_and_values_of(
        Pipeline::from(cake::Entity)
            .filter_with(|binder| cake::Column::Id.gt(binder.bind(3_i32)))
            .join(
                JoinSide::Inner,
                inner,
                cake::Column::Id.eq(alias("cake_id")),
            )
            .select(cake::Column::Name),
    );
    assert_eq!(
        sql,
        "WITH table_1 AS (SELECT name, id FROM cake WHERE id > $1), \
         table_0 AS (SELECT cake_id FROM fruit) \
         SELECT table_1.name FROM table_1 \
         INNER JOIN table_0 ON table_1.id = table_0.cake_id"
    );
    assert_eq!(ints(&values), vec![3_i32.into()]);
}

// [spec:pgorm:req:pipeline.params+3/test]    a pruned placeholder between
// two survivors, one on each side of an embedding
#[test]
fn prune_between_survivors_across_pipelines() {
    let inner = Pipeline::from(fruit::Entity)
        .derive_with(|binder| [binder.bind(5_i32).as_(alias("unused"))])
        .select(fruit::Column::CakeId);
    let (sql, values) = sql_and_values_of(
        Pipeline::from(cake::Entity)
            .filter_with(|binder| cake::Column::Id.gt(binder.bind(3_i32)))
            .join(
                JoinSide::Inner,
                inner,
                cake::Column::Id.eq(alias("cake_id")),
            )
            .filter_with(|binder| cake::Column::Id.lt(binder.bind(100_i32)))
            .select(cake::Column::Name),
    );
    assert_eq!(
        sql,
        "WITH table_1 AS (SELECT name, id FROM cake WHERE id > $1), \
         table_0 AS (SELECT cake_id FROM fruit) \
         SELECT table_1.name FROM table_1 \
         INNER JOIN table_0 ON table_1.id = table_0.cake_id \
         WHERE table_1.id < $2"
    );
    assert_eq!(ints(&values), vec![3_i32.into(), 100_i32.into()]);
}

// [spec:pgorm:req:pipeline.params+3/test]    nesting rides along: the
// innermost pipeline's pruned binding crosses two embeddings before the
// census discards it
#[test]
fn nested_embedding_prunes_through_two_levels() {
    let innermost = Pipeline::from(fruit::Entity)
        .derive_with(|binder| [binder.bind(1_i32).as_(alias("unused"))])
        .select(fruit::Column::CakeId);
    let middle = Pipeline::from(cake::Entity)
        .join(
            JoinSide::Inner,
            innermost,
            cake::Column::Id.eq(alias("cake_id")),
        )
        .filter_with(|binder| cake::Column::Id.gt(binder.bind(2_i32)))
        .select(cake::Column::Id);
    let (sql, values) = sql_and_values_of(
        Pipeline::from(cake::Entity)
            .filter_with(|binder| cake::Column::Id.lt(binder.bind(50_i32)))
            .select(cake::Column::Id)
            .append(middle),
    );
    assert!(sql.contains("UNION ALL"), "{sql}");
    assert!(sql.contains("$1") && sql.contains("$2"), "{sql}");
    assert!(!sql.contains("$3"), "{sql}");
    assert_eq!(ints(&values), vec![50_i32.into(), 2_i32.into()]);
}
