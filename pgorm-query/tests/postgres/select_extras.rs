use super::*;
use crate::oracle::{assert_eq, assert_rejected, parsed_nodes};
use pgorm_query::error::Error;

/// libpg_query's `LimitOption`, by the number it serialises as.
const LIMIT_COUNT: i64 = 2;
const LIMIT_WITH_TIES: i64 = 3;

fn select_stmt(sql: &str) -> serde_json::Value {
    parsed_nodes(sql, "SelectStmt")
        .into_iter()
        .next()
        .expect("a SELECT")
}

fn ordered() -> SelectStatement {
    Query::select()
        .column(Glyph::Image)
        .from(Glyph::Table)
        .order_by(Glyph::Aspect, Order::Desc)
        .take()
}

// [spec:pgorm:req:sql.ast.select.tie-limit+1/test]    the tie limit renders in LIMIT's place and
// parses as PostgreSQL's WITH TIES limit, its count bound
// [spec:pgorm:req:sql.render.tie-limit/test]
#[test]
fn tie_limit_renders_fetch_first_with_ties() {
    let query = ordered().limit_with_ties(3).offset(1).take();
    let sql = query.to_string();
    assert_eq!(
        sql,
        r#"SELECT "image" FROM "glyph" ORDER BY "aspect" DESC FETCH FIRST 3 ROWS WITH TIES OFFSET 1"#
    );
    let stmt = select_stmt(&sql);
    assert_eq!(stmt["limit_option"].as_i64(), Some(LIMIT_WITH_TIES));
    assert!(!stmt["limit_count"].is_null());
    assert!(!stmt["limit_offset"].is_null());

    let (sql, values) = query.build();
    assert_eq!(
        sql,
        r#"SELECT "image" FROM "glyph" ORDER BY "aspect" DESC FETCH FIRST $1 ROWS WITH TIES OFFSET $2"#
    );
    assert_eq!(values.0, vec![3u64.into(), 1u64.into()]);
    assert_eq!(query.check_tie_limit(), Ok(()));
}

// [spec:pgorm:req:sql.ast.select.tie-limit+1/test]    `limit` and `limit_with_ties` share one slot,
// the last call winning, and `reset_limit` clears either
#[test]
fn row_limits_replace_each_other() {
    let plain = ordered().limit_with_ties(3).limit(2).take();
    assert!(!plain.has_tie_limit());
    let sql = plain.to_string();
    assert_eq!(
        sql,
        r#"SELECT "image" FROM "glyph" ORDER BY "aspect" DESC LIMIT 2"#
    );
    assert_eq!(
        select_stmt(&sql)["limit_option"].as_i64(),
        Some(LIMIT_COUNT)
    );

    let ties = ordered().limit(2).limit_with_ties(3).take();
    assert!(ties.has_tie_limit());
    assert_eq!(
        ties.to_string(),
        r#"SELECT "image" FROM "glyph" ORDER BY "aspect" DESC FETCH FIRST 3 ROWS WITH TIES"#
    );

    let cleared = ordered().limit_with_ties(3).reset_limit().take();
    assert!(!cleared.has_tie_limit());
    assert_eq!(
        cleared.to_string(),
        r#"SELECT "image" FROM "glyph" ORDER BY "aspect" DESC"#
    );
}

// [spec:pgorm:req:sql.ast.select.tie-limit+1/test]    `narrow_limit_to_one` narrows every row
// limit to a plain LIMIT of at most one, keeping a limit of zero and the OFFSET
#[test]
fn narrowing_keeps_a_zero_limit() {
    let narrowed = |query: SelectStatement| {
        let narrowed = query.clone().narrow_limit_to_one().take();
        assert!(!narrowed.has_tie_limit());
        let (sql, values) = narrowed.build();
        assert_eq!(
            select_stmt(&sql)["limit_option"].as_i64(),
            Some(LIMIT_COUNT)
        );
        (narrowed.to_string(), values.0)
    };
    let one = r#"SELECT "image" FROM "glyph" ORDER BY "aspect" DESC LIMIT 1"#;
    let none = r#"SELECT "image" FROM "glyph" ORDER BY "aspect" DESC LIMIT 0"#;

    assert_eq!(narrowed(ordered()), (one.to_owned(), vec![1u64.into()]));
    assert_eq!(narrowed(ordered().limit(5).take()).0, one);
    assert_eq!(narrowed(ordered().limit_with_ties(3).take()).0, one);
    assert_eq!(
        narrowed(ordered().limit(0).take()),
        (none.to_owned(), vec![0u64.into()])
    );
    assert_eq!(narrowed(ordered().limit_with_ties(0).take()).0, none);
    assert_eq!(
        narrowed(ordered().limit_with_ties(2).offset(3).take()).0,
        format!("{one} OFFSET 3")
    );
}

// [spec:pgorm:req:sql.ast.select.tie-limit+1/test]    the two shapes the grammar refuses are refused by
// `check_tie_limit`, and the grammar does refuse what they render
// [spec:pgorm:req:sql.render.oracle+2/test]    the one render held rejected by design
// [spec:pgorm:req:sql.render.tie-limit/test]
#[test]
fn ungrammatical_tie_limits_are_refused() {
    let unordered = Query::select()
        .column(Glyph::Image)
        .from(Glyph::Table)
        .limit_with_ties(3)
        .take();
    assert_eq!(unordered.check_tie_limit(), Err(Error::TiesWithoutOrder));
    let message = assert_rejected(&unordered.to_string());
    assert!(message.contains("WITH TIES cannot be specified without ORDER BY clause"));

    let cleared = ordered().limit_with_ties(3).clear_order_by().take();
    assert_eq!(cleared.check_tie_limit(), Err(Error::TiesWithoutOrder));

    let skipping = ordered()
        .limit_with_ties(3)
        .lock_with_behavior(LockType::Update, LockBehavior::SkipLocked)
        .take();
    assert_eq!(skipping.check_tie_limit(), Err(Error::TiesWithSkipLocked));
    let message = assert_rejected(&skipping.to_string());
    assert!(message.contains("SKIP LOCKED and WITH TIES options cannot be used together"));

    let waiting = ordered()
        .limit_with_ties(3)
        .lock_with_behavior(LockType::Update, LockBehavior::Nowait)
        .take();
    assert_eq!(waiting.check_tie_limit(), Ok(()));
    assert_eq!(
        waiting.to_string(),
        r#"SELECT "image" FROM "glyph" ORDER BY "aspect" DESC FETCH FIRST 3 ROWS WITH TIES FOR UPDATE NOWAIT"#
    );

    let plain = Query::select()
        .column(Glyph::Image)
        .from(Glyph::Table)
        .limit(3)
        .take();
    assert_eq!(plain.check_tie_limit(), Ok(()));
}

// [spec:pgorm:req:sql.ast.select.group-distinct/test]    `GROUP BY DISTINCT` over the whole list,
// and nothing when there is no list
// [spec:pgorm:req:sql.render.grouping+1/test]
#[test]
fn group_by_distinct_quantifies_the_whole_list() {
    let sql = Query::select()
        .column(Glyph::Aspect)
        .from(Glyph::Table)
        .group_by_distinct()
        .group_by_element(GroupingElement::rollup([
            Expr::col(Glyph::Aspect),
            Expr::col(Glyph::Image),
        ]))
        .group_by_col(Glyph::Aspect)
        .to_string();
    assert_eq!(
        sql,
        r#"SELECT "aspect" FROM "glyph" GROUP BY DISTINCT ROLLUP ("aspect", "image"), "aspect""#
    );
    let stmt = select_stmt(&sql);
    assert_eq!(stmt["group_distinct"].as_bool(), Some(true));
    assert_eq!(stmt["group_clause"].as_array().map(Vec::len), Some(2));

    let ungrouped = Query::select()
        .column(Glyph::Aspect)
        .from(Glyph::Table)
        .group_by_distinct()
        .to_string();
    assert_eq!(ungrouped, r#"SELECT "aspect" FROM "glyph""#);
}

/// libpg_query's `A_Expr_Kind` for `SIMILAR TO`, by the number it serialises
/// as.
const AEXPR_SIMILAR: i64 = 10;

/// The `A_Expr` nodes of `sql`, as `(kind, operator)`.
fn a_exprs(sql: &str) -> Vec<(i64, String)> {
    parsed_nodes(sql, "AExpr")
        .iter()
        .map(|expr| {
            (
                expr["kind"].as_i64().unwrap_or_default(),
                expr["name"][0]["String"]["sval"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
        .collect()
}

// [spec:pgorm:req:sql.ast.expr.similar-to+1/test]    both forms, the pattern bound and an ESCAPE
// inline, and the grammar reads them as SIMILAR TO
// [spec:pgorm:def:sql.render.operators+7/test]
#[test]
fn similar_to_binds_its_pattern() {
    let query = Query::select()
        .column(Char::Character)
        .from(Char::Table)
        .and_where(Expr::col(Char::Character).similar_to("(a|b)%"))
        .and_where(Expr::col(Char::Character).not_similar_to(LikeExpr::new("!%%").escape('!')))
        .take();

    let (sql, values) = query.build();
    assert_eq!(
        sql,
        r#"SELECT "character" FROM "character" WHERE "character" SIMILAR TO $1 AND "character" NOT SIMILAR TO $2 ESCAPE '!'"#
    );
    assert_eq!(values.0, vec!["(a|b)%".into(), "!%%".into()]);

    let sql = query.to_string();
    assert_eq!(
        sql,
        r#"SELECT "character" FROM "character" WHERE "character" SIMILAR TO '(a|b)%' AND "character" NOT SIMILAR TO '!%%' ESCAPE '!'"#
    );
    assert_eq!(
        a_exprs(&sql),
        [
            (AEXPR_SIMILAR, "~".to_owned()),
            (AEXPR_SIMILAR, "!~".to_owned())
        ]
    );
    // The grammar wraps every SIMILAR TO pattern in `similar_to_escape`, whose
    // second argument is the ESCAPE when there is one.
    let patterns = parsed_nodes(&sql, "FuncCall")
        .iter()
        .map(|call| call["args"].as_array().map_or(0, Vec::len))
        .collect::<Vec<_>>();
    assert_eq!(patterns, [1, 2]);
}

// [spec:pgorm:req:sql.ast.expr.similar-to+1/test]    `no_escape` renders `ESCAPE ''` after LIKE,
// ILIKE and SIMILAR TO's bound pattern, and the grammar reads an empty escape there
// [spec:pgorm:def:sql.render.operators+7/test]
#[test]
fn no_escape_renders_an_empty_escape() {
    let off = || LikeExpr::new(r"a\%").no_escape();
    let query = Query::select()
        .column(Char::Character)
        .from(Char::Table)
        .and_where(Expr::col(Char::Character).like(off()))
        .and_where(Expr::col(Char::Character).ilike(off()))
        .and_where(Expr::col(Char::Character).similar_to(off()))
        .take();

    let (sql, values) = query.build();
    assert_eq!(
        sql,
        r#"SELECT "character" FROM "character" WHERE "character" LIKE $1 ESCAPE '' AND ("character" ILIKE $2 ESCAPE '') AND "character" SIMILAR TO $3 ESCAPE ''"#
    );
    assert_eq!(values.0, vec![r"a\%".into(); 3]);

    // The grammar folds an ESCAPE into a call on the pattern: `like_escape`
    // for LIKE and ILIKE, `similar_to_escape` for SIMILAR TO, the escape its
    // second argument.
    let sql = query.to_string();
    let escapes: Vec<(String, String)> = parsed_nodes(&sql, "AExpr")
        .iter()
        .map(|expr| {
            let call = &expr["rexpr"]["FuncCall"];
            (
                call["funcname"][1]["String"]["sval"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                call["args"][1]["AConst"]["val"]["Sval"]["sval"]
                    .as_str()
                    .unwrap_or("no escape")
                    .to_owned(),
            )
        })
        .collect();
    let empty = |call: &str| (call.to_owned(), String::new());
    assert_eq!(
        escapes,
        [
            empty("like_escape"),
            empty("like_escape"),
            empty("similar_to_escape")
        ]
    );
}

// [spec:pgorm:req:sql.ast.expr.similar-to+1/test]    SIMILAR TO ranks with LIKE: bare under the
// logical operators and NOT, an arithmetic operand bare inside it
// [spec:pgorm:def:sql.render.precedence+9/test]
#[test]
fn similar_to_sits_in_the_like_family() {
    let sql = Query::select()
        .column(Char::Character)
        .from(Char::Table)
        .cond_where(
            Condition::any()
                .add(Expr::col(Char::Character).similar_to("a%"))
                .add(Expr::col(Char::Character).not_similar_to("b%")),
        )
        .and_where(Expr::col(Char::SizeW).add(1).similar_to("1%"))
        .and_where(Expr::col(Char::FontId).similar_to("2%").not())
        .to_string();
    assert_eq!(
        sql,
        r#"SELECT "character" FROM "character" WHERE ("character" SIMILAR TO 'a%' OR "character" NOT SIMILAR TO 'b%') AND "size_w" + 1 SIMILAR TO '1%' AND (NOT "font_id" SIMILAR TO '2%')"#
    );
}

fn series(from: i32, to: i32) -> FunctionCall {
    Func::named(Name::runtime("generate_series"))
        .arg(from)
        .arg(to)
}

fn range_functions(sql: &str) -> Vec<serde_json::Value> {
    parsed_nodes(sql, "RangeFunction")
}

// [spec:pgorm:req:sql.ast.select.table-function/test]    `WITH ORDINALITY` on one call, with
// column names in the alias
// [spec:pgorm:req:sql.render.table-function/test]
#[test]
fn ordinality_numbers_one_function() {
    let sql = Query::select()
        .column(Asterisk)
        .from(
            TableFunction::new(series(10, 12))
                .with_ordinality()
                .columns([Name::runtime("v"), Name::runtime("n")])
                .alias(Name::runtime("t")),
        )
        .to_string();
    assert_eq!(
        sql,
        r#"SELECT * FROM generate_series(10, 12) WITH ORDINALITY AS "t"("v", "n")"#
    );
    let functions = range_functions(&sql);
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0]["ordinality"].as_bool(), Some(true));
    assert_eq!(functions[0]["is_rowsfrom"].as_bool(), Some(false));
    assert_eq!(
        functions[0]["alias"]["colnames"].as_array().map(Vec::len),
        Some(2)
    );
}

// [spec:pgorm:req:sql.ast.select.table-function/test]    `ROWS FROM` over several calls, a lone
// `rows_from`, and the plain call that `alias` returns as `FromItem::FunctionCall`
// [spec:pgorm:req:sql.render.table-function/test]
#[test]
fn rows_from_zips_several_functions() {
    let sql = Query::select()
        .column(Asterisk)
        .from(
            TableFunction::new(series(1, 2))
                .and(series(1, 3))
                .with_ordinality()
                .alias(Name::runtime("t")),
        )
        .to_string();
    assert_eq!(
        sql,
        r#"SELECT * FROM ROWS FROM (generate_series(1, 2), generate_series(1, 3)) WITH ORDINALITY AS "t""#
    );
    let functions = range_functions(&sql);
    assert_eq!(functions[0]["is_rowsfrom"].as_bool(), Some(true));
    assert_eq!(functions[0]["functions"].as_array().map(Vec::len), Some(2));

    let sql = Query::select()
        .column(Asterisk)
        .from(TableFunction::rows_from(series(1, 2)).alias(Name::runtime("t")))
        .to_string();
    assert_eq!(
        sql,
        r#"SELECT * FROM ROWS FROM (generate_series(1, 2)) AS "t""#
    );
    assert_eq!(
        range_functions(&sql)[0]["is_rowsfrom"].as_bool(),
        Some(true)
    );

    let plain = TableFunction::new(series(1, 2)).alias(Name::runtime("t"));
    assert_eq!(
        plain,
        FromItem::FunctionCall(series(1, 2), Name::runtime("t"))
    );
    assert_eq!(plain.qualifier().to_string(), "t");
}

// [spec:pgorm:req:sql.ast.select.table-sample/test]    both methods, an alias and a schema on the
// sampled table, the percentage and seed bound, in a join and in an UPDATE's FROM list
// [spec:pgorm:req:sql.render.table-sample/test]
#[test]
fn tablesample_binds_percentage_and_seed() {
    let sample = TableSample::new(
        (Name::runtime("public"), Glyph::Table)
            .into_named_table()
            .alias(Name::runtime("g")),
        SampleMethod::System,
        12.5,
    )
    .repeatable(42);
    let query = Query::select()
        .column((Name::runtime("g"), Glyph::Image))
        .from(sample.clone())
        .take();

    let (sql, values) = query.build();
    assert_eq!(
        sql,
        r#"SELECT "g"."image" FROM "public"."glyph" AS "g" TABLESAMPLE SYSTEM ($1) REPEATABLE ($2)"#
    );
    assert_eq!(values.0, vec![12.5f64.into(), 42f64.into()]);

    let sql = query.to_string();
    let samples = parsed_nodes(&sql, "RangeTableSample");
    assert_eq!(samples.len(), 1);
    assert_eq!(
        samples[0]["method"][0]["String"]["sval"].as_str(),
        Some("system")
    );
    assert_eq!(samples[0]["args"].as_array().map(Vec::len), Some(1));
    assert!(samples[0].get("repeatable").is_some());

    let item = FromItem::from(sample);
    assert_eq!(item.qualifier().to_string(), "g");
    assert_eq!(
        item.table_name().map(|name| name.table().to_string()),
        Some("glyph".to_owned())
    );

    let sql = Query::select()
        .column(Glyph::Image)
        .from(TableSample::new(Glyph::Table, SampleMethod::Bernoulli, 50))
        .inner_join(
            Font::Table,
            Expr::col((Glyph::Table, Glyph::Id)).equals((Font::Table, Font::Id)),
        )
        .to_string();
    assert_eq!(
        sql,
        r#"SELECT "image" FROM "glyph" TABLESAMPLE BERNOULLI (50) INNER JOIN "font" ON "glyph"."id" = "font"."id""#
    );
    let samples = parsed_nodes(&sql, "RangeTableSample");
    assert_eq!(
        samples[0]["method"][0]["String"]["sval"].as_str(),
        Some("bernoulli")
    );
    assert_eq!(samples[0].get("repeatable"), None);

    let update = Query::update()
        .table(Font::Table)
        .value(Font::Name, "x")
        .from(TableSample::new(Glyph::Table, SampleMethod::Bernoulli, 50))
        .and_where(Expr::col((Font::Table, Font::Id)).equals((Glyph::Table, Glyph::Id)))
        .to_string();
    assert_eq!(
        update,
        r#"UPDATE "font" SET "name" = 'x' FROM "glyph" TABLESAMPLE BERNOULLI (50) WHERE "font"."id" = "glyph"."id""#
    );
    assert_eq!(parsed_nodes(&update, "RangeTableSample").len(), 1);

    let realiased = FromItem::from(TableSample::new(Glyph::Table, SampleMethod::Bernoulli, 50))
        .alias(Name::runtime("s"));
    assert_eq!(realiased.qualifier().to_string(), "s");
}

// [spec:pgorm:req:sql.ast.select.table-function/test]    the set-returning constructors, their
// arguments bound
// [spec:pgorm:def:sql.ast.func+10/test]
#[test]
fn set_returning_constructors_bind_arguments() {
    let (sql, values) = Query::select()
        .column(Asterisk)
        .from(
            TableFunction::rows_from(Func::generate_series_step(10, 1, -3))
                .and(Func::unnest(Expr::col(Name::runtime("tags"))))
                .alias(Name::runtime("t")),
        )
        .build();
    assert_eq!(
        sql,
        r#"SELECT * FROM ROWS FROM (GENERATE_SERIES($1, $2, $3), UNNEST("tags")) AS "t""#
    );
    assert_eq!(values.0, vec![10.into(), 1.into(), (-3).into()]);

    let call = Func::generate_series(1, 3);
    assert_eq!(call.get_func(), &Function::GenerateSeries);
    assert_eq!(call.get_args().len(), 2);
    assert_eq!(
        Func::unnest(Expr::col(Name::runtime("tags"))).get_func(),
        &Function::Unnest
    );
}
