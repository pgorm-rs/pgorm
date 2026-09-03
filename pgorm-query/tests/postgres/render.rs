use super::*;
use crate::oracle::assert_eq;

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
    let with = select().with(WithClause::new(CommonTableExpression::new(
        Alias::new("cte"),
        select(),
    )));

    assert_eq!(
        select().to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph" WHERE "aspect" = 1"#
    );
    assert_eq!(
        insert.to_string(QueryBuilder),
        r#"INSERT INTO "glyph" ("aspect") VALUES (1)"#
    );
    assert_eq!(
        update.to_string(QueryBuilder),
        r#"UPDATE "glyph" SET "aspect" = 1"#
    );
    assert_eq!(
        delete.to_string(QueryBuilder),
        r#"DELETE FROM "glyph" WHERE "id" = 1"#
    );
    assert_eq!(
        with.to_string(QueryBuilder),
        [
            r#"WITH "cte" AS (SELECT "id" FROM "glyph" WHERE "aspect" = 1)"#,
            r#"SELECT "id" FROM "glyph" WHERE "aspect" = 1"#,
        ]
        .join(" ")
    );

    // DDL goes through the very same builder.
    assert_eq!(
        Table::truncate(Glyph::Table).to_string(QueryBuilder),
        r#"TRUNCATE TABLE "glyph""#
    );

    // And every one of them can be written into an arbitrary `SqlWriter` sink
    // through the generic `build_collect_any_into` entry point.
    fn collect<S: QueryStatementBuilder>(statement: &S) -> String {
        let mut sql = SqlWriterValues::new("$", true);
        statement.build_collect_any_into(&QueryBuilder, &mut sql);
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
}

// [spec:pgorm:def:sql.render/test]    rendering is infallible by construction: a DDL target is a
// `TableName`, which has no shape the renderer would have to refuse
#[test]
fn ddl_targets_have_no_unrenderable_shape() {
    for name in [
        Glyph::Table.into_table_name(),
        (Alias::new("public"), Glyph::Table).into_table_name(),
    ] {
        let sql = Table::truncate(name).to_string(QueryBuilder);
        assert!(sql.starts_with("TRUNCATE TABLE "), "{sql}");
    }
}

// [spec:pgorm:def:sql.render.writer+1/test]    `String` is the inline-rendering sink: `push_param`
// appends the value as a literal, and it takes the default `push_param_source_typed`
#[test]
fn string_sink_renders_parameters_inline() {
    let mut sql = String::new();
    write!(sql, "SELECT ").unwrap();
    sql.push_param(Value::Int(Some(3)), &QueryBuilder);
    write!(sql, ", ").unwrap();
    sql.push_param(Value::String(Some(Box::new("a".to_owned()))), &QueryBuilder);
    assert_eq!(sql, "SELECT 3, 'a'");

    // An inline literal has no wire format to disagree about, so the source-typed
    // push is the plain one.
    let mut typed = String::new();
    typed.push_param_source_typed(Value::BigInt(Some(8)), &QueryBuilder);
    let mut plain = String::new();
    plain.push_param(Value::BigInt(Some(8)), &QueryBuilder);
    assert_eq!(typed, plain);
    assert_eq!(typed, "8");
}

// [spec:pgorm:def:sql.render.writer+1/test]    `SqlWriterValues` emits a placeholder and collects
// the value; `into_parts` hands back the `(String, Values)` pair
#[test]
fn values_sink_emits_placeholders_and_collects_values() {
    let mut sql = SqlWriterValues::new("$", true);
    write!(sql, "SELECT ").unwrap();
    sql.push_param(Value::Int(Some(3)), &QueryBuilder);
    write!(sql, ", ").unwrap();
    sql.push_param(Value::String(Some(Box::new("a".to_owned()))), &QueryBuilder);
    write!(sql, ", ").unwrap();
    sql.push_param_source_typed(Value::BigInt(Some(8)), &QueryBuilder);

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

// [spec:pgorm:def:sql.render.writer+1/test]    the sink is constructed with a placeholder string
// and a `numbered` flag
#[test]
fn values_sink_honours_its_placeholder_and_numbered_flag() {
    let mut sql = SqlWriterValues::new("?", false);
    sql.push_param(Value::Int(Some(1)), &QueryBuilder);
    write!(sql, " ").unwrap();
    sql.push_param(Value::Int(Some(2)), &QueryBuilder);
    let (rendered, values) = sql.into_parts();
    assert_eq!(rendered, "? ?");
    assert_eq!(
        values,
        Values(vec![Value::Int(Some(1)), Value::Int(Some(2))])
    );

    let mut sql = SqlWriterValues::new(":", true);
    sql.push_param(Value::Int(Some(1)), &QueryBuilder);
    write!(sql, " ").unwrap();
    sql.push_param(Value::Int(Some(2)), &QueryBuilder);
    let (rendered, _) = sql.into_parts();
    assert_eq!(rendered, ":1 :2");
}

// [spec:pgorm:def:sql.render.writer+1/test]    both sinks are usable through `&mut dyn SqlWriter`,
// which is what lets one statement render either way
#[test]
fn both_sinks_are_reachable_through_the_trait_object() {
    let statement = select();

    let mut inline: String = String::new();
    let mut collected = SqlWriterValues::new("$", true);

    let sinks: [&mut dyn SqlWriter; 2] = [&mut inline, &mut collected];
    for sink in sinks {
        statement.build_collect_any_into(&QueryBuilder, sink);
    }

    assert_eq!(inline, r#"SELECT "id" FROM "glyph" WHERE "aspect" = 1"#);
    let (sql, values) = collected.into_parts();
    assert_eq!(sql, r#"SELECT "id" FROM "glyph" WHERE "aspect" = $1"#);
    assert_eq!(values, Values(vec![Value::Int(Some(1))]));
}
