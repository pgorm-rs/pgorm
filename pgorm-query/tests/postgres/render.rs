use super::*;
use crate::oracle::{assert_eq, assert_eq_unparsed};
use pgorm_query::error::{Error, TemplateError};

fn select() -> SelectStatement {
    Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .and_where(Expr::col(Glyph::Aspect).eq(1))
        .take()
}

// [spec:pgorm:def:sql.render/test]    `QueryBuilder` is the one backend, and it walks every
// statement kind of the AST into a `SqlWriter` sink
#[test]
fn the_single_backend_renders_every_statement_kind() {
    let insert = Query::insert()
        .into_table(Glyph::Table)
        .columns([Glyph::Aspect])
        .values_panic([1.into()])
        .to_owned();
    let update = Query::update()
        .table(Glyph::Table)
        .value(Glyph::Aspect, 1)
        .to_owned();
    let delete = Query::delete()
        .from_table(Glyph::Table)
        .and_where(Expr::col(Glyph::Id).eq(1))
        .to_owned();
    let cte = || CommonTableExpression::new(Alias::new("cte"), select());
    let with = select().with(WithClause::new(cte()));
    let with_query = WithClause::new(cte()).query(delete.clone());

    assert_eq!(
        select().to_string(),
        r#"SELECT "id" FROM "glyph" WHERE "aspect" = 1"#
    );
    assert_eq!(
        insert.to_string(),
        r#"INSERT INTO "glyph" ("aspect") VALUES (1)"#
    );
    assert_eq!(update.to_string(), r#"UPDATE "glyph" SET "aspect" = 1"#);
    assert_eq!(delete.to_string(), r#"DELETE FROM "glyph" WHERE "id" = 1"#);
    assert_eq!(
        with.to_string(),
        [
            r#"WITH "cte" AS (SELECT "id" FROM "glyph" WHERE "aspect" = 1)"#,
            r#"SELECT "id" FROM "glyph" WHERE "aspect" = 1"#,
        ]
        .join(" ")
    );
    assert_eq!(
        with_query.to_string(),
        [
            r#"WITH "cte" AS (SELECT "id" FROM "glyph" WHERE "aspect" = 1)"#,
            r#"DELETE FROM "glyph" WHERE "id" = 1"#,
        ]
        .join(" ")
    );

    // DDL goes through the very same builder.
    assert_eq!(
        Table::truncate(Glyph::Table).to_string(),
        r#"TRUNCATE TABLE "glyph""#
    );

    // And every one of them can be written into an arbitrary `SqlWriter` sink
    // through the generic `build_collect_into` entry point.
    fn collect<S: QueryStatementBuilder>(statement: &S) -> String {
        let mut sql = SqlWriterValues::new("$", true);
        statement.build_collect_into(&mut sql);
        let (sql, _) = sql.into_parts();
        sql
    }

    assert_eq!(
        collect(&select()),
        r#"SELECT "id" FROM "glyph" WHERE "aspect" = $1"#
    );
    assert_eq!(
        collect(&insert),
        r#"INSERT INTO "glyph" ("aspect") VALUES ($1)"#
    );
    assert_eq!(collect(&update), r#"UPDATE "glyph" SET "aspect" = $1"#);
    assert_eq!(collect(&delete), r#"DELETE FROM "glyph" WHERE "id" = $1"#);
    assert_eq!(
        collect(&with),
        [
            r#"WITH "cte" AS (SELECT "id" FROM "glyph" WHERE "aspect" = $1)"#,
            r#"SELECT "id" FROM "glyph" WHERE "aspect" = $2"#,
        ]
        .join(" ")
    );
    assert_eq!(
        collect(&with_query),
        [
            r#"WITH "cte" AS (SELECT "id" FROM "glyph" WHERE "aspect" = $1)"#,
            r#"DELETE FROM "glyph" WHERE "id" = $2"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:def:sql.render/test]    rendering is infallible by construction: a DDL target is a
// `TableName`, which has no shape the renderer would have to refuse
#[test]
fn ddl_targets_have_no_unrenderable_shape() {
    for name in [
        Glyph::Table.into_table_name(),
        (Alias::new("public"), Glyph::Table).into_table_name(),
    ] {
        let sql = Table::truncate(name).to_string();
        assert!(sql.starts_with("TRUNCATE TABLE "), "{sql}");
    }
}

// [spec:pgorm:def:sql.render.writer+2/test]    `String` is the inline-rendering sink: `push_param`
// appends the value as a literal, and it takes the default `push_param_source_typed`
#[test]
fn string_sink_renders_parameters_inline() {
    let mut sql = String::new();
    write!(sql, "SELECT ").unwrap();
    sql.push_param(Value::Int(Some(3)));
    write!(sql, ", ").unwrap();
    sql.push_param(Value::String(Some(Box::new("a".to_owned()))));
    assert_eq!(sql, "SELECT 3, 'a'");

    // An inline literal has no wire format to disagree about, so the source-typed
    // push is the plain one.
    let mut typed = String::new();
    typed.push_param_source_typed(Value::BigInt(Some(8)));
    let mut plain = String::new();
    plain.push_param(Value::BigInt(Some(8)));
    assert_eq!(typed, plain);
    assert_eq!(typed, "8");
}

// [spec:pgorm:def:sql.render.writer+2/test]    `SqlWriterValues` emits a placeholder and collects
// the value; `into_parts` hands back the `(String, Values)` pair
#[test]
fn values_sink_emits_placeholders_and_collects_values() {
    let mut sql = SqlWriterValues::new("$", true);
    write!(sql, "SELECT ").unwrap();
    sql.push_param(Value::Int(Some(3)));
    write!(sql, ", ").unwrap();
    sql.push_param(Value::String(Some(Box::new("a".to_owned()))));
    write!(sql, ", ").unwrap();
    sql.push_param_source_typed(Value::BigInt(Some(8)));

    // `Display`/`ToString` shows the SQL accumulated so far.
    assert_eq!(sql.to_string(), "SELECT $1, $2, $3::int8");

    let (sql, values) = sql.into_parts();
    assert_eq!(sql, "SELECT $1, $2, $3::int8");
    assert_eq!(
        values,
        Values(vec![
            Value::Int(Some(3)),
            Value::String(Some(Box::new("a".to_owned()))),
            Value::BigInt(Some(8)),
        ])
    );
}

// [spec:pgorm:def:sql.render.writer+2/test]    the sink is constructed with a placeholder string
// and a `numbered` flag
#[test]
fn values_sink_honours_its_placeholder_and_numbered_flag() {
    let mut sql = SqlWriterValues::new("?", false);
    sql.push_param(Value::Int(Some(1)));
    write!(sql, " ").unwrap();
    sql.push_param(Value::Int(Some(2)));
    let (rendered, values) = sql.into_parts();
    assert_eq!(rendered, "? ?");
    assert_eq!(
        values,
        Values(vec![Value::Int(Some(1)), Value::Int(Some(2))])
    );

    let mut sql = SqlWriterValues::new(":", true);
    sql.push_param(Value::Int(Some(1)));
    write!(sql, " ").unwrap();
    sql.push_param(Value::Int(Some(2)));
    let (rendered, _) = sql.into_parts();
    assert_eq!(rendered, ":1 :2");
}

// [spec:pgorm:def:sql.render.writer+2/test]    both sinks are usable through `&mut dyn SqlWriter`,
// which is what lets one statement render either way
#[test]
fn both_sinks_are_reachable_through_the_trait_object() {
    let statement = select();

    let mut inline: String = String::new();
    let mut collected = SqlWriterValues::new("$", true);

    let sinks: [&mut dyn SqlWriter; 2] = [&mut inline, &mut collected];
    for sink in sinks {
        statement.build_collect_into(sink);
    }

    assert_eq!(inline, r#"SELECT "id" FROM "glyph" WHERE "aspect" = 1"#);
    let (sql, values) = collected.into_parts();
    assert_eq!(sql, r#"SELECT "id" FROM "glyph" WHERE "aspect" = $1"#);
    assert_eq!(values, Values(vec![Value::Int(Some(1))]));
}

fn selecting(expr: SimpleExpr) -> String {
    Query::select().expr(expr).to_string()
}

// [spec:pgorm:req:sql.render.custom-expr+1/test]    `$N` names the Nth value counting from one,
// and may be written as often as the template likes
#[test]
fn custom_expr_indices_start_at_one_and_repeat() {
    assert_eq!(
        selecting(Expr::cust_with_values("6 = $1 * $2", [2, 3]).expect("template arity")),
        "SELECT 6 = 2 * 3"
    );
    assert_eq!(
        selecting(Expr::cust_with_values("$2 * $1", [2, 3]).expect("template arity")),
        "SELECT 3 * 2"
    );
    assert_eq!(
        selecting(Expr::cust_with_values("$1 + $1 + $1", [7]).expect("template arity")),
        "SELECT 7 + 7 + 7"
    );
}

// [spec:pgorm:req:sql.render.custom-expr+1/test]    `$$` writes one literal `$`, and quoted
// regions are opaque to the tokenizer so placeholder-shaped text inside them is left alone
#[test]
fn custom_expr_escape_and_quoted_regions_survive_untouched() {
    // A lone `$` is not a PostgreSQL operator, so this one is held to the
    // escape's own contract rather than to the render oracle.
    assert_eq_unparsed!(
        selecting(Expr::cust_with_values("$1 $$ $2", ["a", "b"]).expect("template arity")),
        "SELECT 'a' $ 'b'"
    );
    assert_eq!(
        selecting(Expr::cust_with_values("'$2' || $1", ["a"]).expect("template arity")),
        "SELECT '$2' || 'a'"
    );
}

// [spec:pgorm:req:sql.render.custom-expr+1/test]    a template naming more values than it was
// given is refused where it is written, rather than indexing off the end at render
#[test]
fn custom_expr_under_supply_is_refused_at_construction() {
    assert_eq!(
        Expr::cust_with_values("6 = $1 * $2", [2]).unwrap_err(),
        Error::Template {
            template: "6 = $1 * $2".to_owned(),
            reason: TemplateError::IndexOutOfRange {
                index: 2,
                supplied: 1
            },
        }
    );
    assert!(Expr::cust_with_exprs("$1 + $2", [Expr::val(1).into()]).is_err());
    assert!(Expr::cust_with_expr("$1 + $2", Expr::val(1)).is_err());
}

// [spec:pgorm:req:sql.render.custom-expr+1/test]    a value the template never names is refused
// too: silently dropping it would render something the caller did not write
#[test]
fn custom_expr_over_supply_is_refused_at_construction() {
    assert_eq!(
        Expr::cust_with_values("6 = $1", [2, 3]).unwrap_err(),
        Error::Template {
            template: "6 = $1".to_owned(),
            reason: TemplateError::UnreferencedValue {
                index: 2,
                supplied: 2
            },
        }
    );
    assert!(Expr::cust_with_expr("now()", Expr::val(1)).is_err());
}

// [spec:pgorm:req:sql.render.custom-expr+1/test]    the census demands exactly `1..=len`, so a
// hole in the numbering is a refusal even when the count happens to line up
#[test]
fn custom_expr_arity_hole_is_refused_at_construction() {
    assert_eq!(
        Expr::cust_with_values("$1 + $3", [1, 2, 3]).unwrap_err(),
        Error::Template {
            template: "$1 + $3".to_owned(),
            reason: TemplateError::UnreferencedValue {
                index: 2,
                supplied: 3
            },
        }
    );
}

// [spec:pgorm:req:sql.render.custom-expr+1/test]    `$0` names nothing, and a `$` that is neither
// an escape nor an index is a malformed placeholder rather than silent text loss
#[test]
fn zero_and_malformed_placeholders_are_refused() {
    assert_eq!(
        Expr::cust_with_values("$0", [1]).unwrap_err(),
        Error::Template {
            template: "$0".to_owned(),
            reason: TemplateError::ZeroIndex,
        }
    );
    assert_eq!(
        Expr::cust_with_values("$abc", [1]).unwrap_err(),
        Error::Template {
            template: "$abc".to_owned(),
            reason: TemplateError::MalformedPlaceholder { position: 0 },
        }
    );
    assert!(Expr::cust_with_values("a $ b", [1]).is_err());
    assert!(Expr::cust_with_values("$1 $", [1]).is_err());
}

// [spec:pgorm:req:sql.render.custom-expr+1/test]    a template with no placeholders and no values
// is a complete pair, and renders verbatim
#[test]
fn custom_expr_empty_census_is_a_complete_pair() {
    assert_eq!(
        selecting(Expr::cust_with_exprs("now()", []).expect("template arity")),
        "SELECT now()"
    );
}

// [spec:pgorm:req:sql.render.custom-expr+1/test]    every template that survives construction
// renders, through either sink, without reaching for a value it does not hold
#[test]
fn custom_expr_that_was_constructed_always_renders() {
    let expr = Expr::cust_with_values("$1 $$ $2 || $1", ["a", "b"]).expect("template arity");

    assert_eq_unparsed!(selecting(expr.clone()), "SELECT 'a' $ 'b' || 'a'");

    let (sql, values) = Query::select().expr(expr).build();
    assert_eq_unparsed!(sql, "SELECT $1 $ $2 || $3");
    assert_eq!(values, Values(vec!["a".into(), "b".into(), "a".into()]));
}
