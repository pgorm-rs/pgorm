use super::*;
use crate::oracle::{assert_eq, parsed_nodes};
use CmdType::{CmdDelete, CmdInsert, CmdNothing, CmdUpdate};
use MergeMatchKind::{MergeWhenMatched as Matched, MergeWhenNotMatchedByTarget as NotMatched};
use pg_query::protobuf::{BoolExprType, CmdType, MergeMatchKind, OverridingKind};

fn on() -> SimpleExpr {
    Expr::col((Glyph::Table, Glyph::Id)).equals((Font::Table, Font::Id))
}

fn merge() -> PendingMerge {
    Query::merge(Glyph::Table, Font::Table, on())
}

fn head() -> &'static str {
    r#"MERGE INTO "glyph" USING "font" ON "glyph"."id" = "font"."id""#
}

fn font(column: Font) -> Expr {
    Expr::col((Font::Table, column))
}

/// The one `MergeStmt` in `sql`, as PostgreSQL's parser read it.
fn statement(sql: &str) -> serde_json::Value {
    let mut found = parsed_nodes(sql, "MergeStmt");
    assert_eq!(found.len(), 1, "exactly one MERGE in {sql}");
    found.remove(0)
}

fn code(clause: &serde_json::Value, field: &str) -> i32 {
    let code = clause[field]
        .as_i64()
        .unwrap_or_else(|| panic!("a {field} code in {clause}"));
    <i32 as TryFrom<i64>>::try_from(code).expect("a protobuf enum code fits i32")
}

/// Each `WHEN` clause of `sql` in the order the parser read them: which rows
/// it takes, what it does to them, and whether it carries an `AND` condition.
fn arms(sql: &str) -> Vec<(MergeMatchKind, CmdType, bool)> {
    parsed_nodes(sql, "MergeWhenClause")
        .iter()
        .map(|clause| {
            let kind = MergeMatchKind::try_from(code(clause, "match_kind")).expect("a match kind");
            let command = CmdType::try_from(code(clause, "command_type")).expect("a command");
            (kind, command, clause.get("condition").is_some())
        })
        .collect()
}

// [spec:pgorm:req:sql.ast.merge/test]    the target, the source and the condition, then arms
// [spec:pgorm:req:sql.render.merge/test]    `MERGE INTO <target> USING <source> ON <condition>`
#[test]
fn merge_renders_target_source_condition_and_arms() {
    let sql = merge()
        .when_matched(MergeUpdate::value(Glyph::Image, font(Font::Name)))
        .when_not_matched(
            MergeInsert::value(Glyph::Id, font(Font::Id)).and_value(Glyph::Image, font(Font::Name)),
        )
        .to_string();
    assert_eq!(
        sql,
        [
            head(),
            r#"WHEN MATCHED THEN UPDATE SET "image" = "font"."name""#,
            r#"WHEN NOT MATCHED THEN INSERT ("id", "image") VALUES ("font"."id", "font"."name")"#,
        ]
        .join(" ")
    );
    let merge = statement(&sql);
    assert_eq!(merge["relation"]["relname"], "glyph");
    assert_eq!(merge["source_relation"]["RangeVar"]["relname"], "font");
    assert!(merge.get("join_condition").is_some(), "{sql}");
    assert_eq!(
        arms(&sql),
        [(Matched, CmdUpdate, false), (NotMatched, CmdInsert, false)]
    );
}

// [spec:pgorm:req:sql.ast.merge/test]    conditional arms are tried in the order they were added
// [spec:pgorm:req:sql.render.merge/test]
#[test]
fn conditional_arms_keep_the_order_they_were_added() {
    let gone = || Expr::col((Font::Table, Font::Name)).is_null();
    let blank = || Expr::col((Glyph::Table, Glyph::Image)).is_null();
    let rename = || MergeUpdate::value(Glyph::Image, font(Font::Name));

    let delete_first = merge()
        .when_matched_and(gone(), MatchedAction::Delete)
        .when_matched_and(blank(), rename())
        .to_string();
    assert_eq!(
        delete_first,
        [
            head(),
            r#"WHEN MATCHED AND "font"."name" IS NULL THEN DELETE"#,
            r#"WHEN MATCHED AND "glyph"."image" IS NULL THEN UPDATE SET "image" = "font"."name""#,
        ]
        .join(" ")
    );
    assert_eq!(
        arms(&delete_first),
        [(Matched, CmdDelete, true), (Matched, CmdUpdate, true)]
    );

    let update_first = merge()
        .when_matched_and(blank(), rename())
        .when_matched_and(gone(), MatchedAction::Delete)
        .to_string();
    assert_eq!(
        arms(&update_first),
        [(Matched, CmdUpdate, true), (Matched, CmdDelete, true)]
    );
}

// [spec:pgorm:req:sql.ast.merge/test]    the unreachable-arm statement has no representation: an
// unconditional arm renders after its kind's conditional arms whenever it was added
// [spec:pgorm:req:sql.render.merge/test]
#[test]
fn unconditional_arm_renders_after_conditional_arms() {
    let sql = merge()
        .when_matched(MatchedAction::Delete)
        .when_matched_and(font(Font::Name).is_null(), MatchedAction::DoNothing)
        .when_not_matched(NotMatchedAction::DoNothing)
        .when_not_matched_and(
            font(Font::Language).eq("en"),
            MergeInsert::value(Glyph::Id, font(Font::Id)),
        )
        .to_string();
    assert_eq!(
        sql,
        [
            head(),
            r#"WHEN MATCHED AND "font"."name" IS NULL THEN DO NOTHING"#,
            r#"WHEN MATCHED THEN DELETE"#,
            r#"WHEN NOT MATCHED AND "font"."language" = 'en' THEN INSERT ("id") VALUES ("font"."id")"#,
            r#"WHEN NOT MATCHED THEN DO NOTHING"#,
        ]
        .join(" ")
    );
    assert_eq!(
        arms(&sql),
        [
            (Matched, CmdNothing, true),
            (Matched, CmdDelete, false),
            (NotMatched, CmdInsert, true),
            (NotMatched, CmdNothing, false),
        ]
    );
}

// [spec:pgorm:req:sql.render.merge/test]    an arm's condition takes no parentheses: `AND` is
// the clause's keyword, and a top-level OR after it is read as the whole condition
#[test]
fn an_arms_condition_is_read_whole() {
    let sql = merge()
        .when_matched_and(
            Condition::any()
                .add(font(Font::Name).is_null())
                .add(font(Font::Variant).eq("gone")),
            MatchedAction::Delete,
        )
        .to_string();
    assert_eq!(
        sql,
        format!(
            "{} {}",
            head(),
            r#"WHEN MATCHED AND "font"."name" IS NULL OR "font"."variant" = 'gone' THEN DELETE"#
        )
    );
    let clauses = parsed_nodes(&sql, "MergeWhenClause");
    let condition = &clauses[0]["condition"]["BoolExpr"];
    assert_eq!(
        BoolExprType::try_from(code(condition, "boolop")).expect("a boolean operator"),
        BoolExprType::OrExpr,
        "{sql}"
    );
    assert_eq!(condition["args"].as_array().map(Vec::len), Some(2), "{sql}");
}

// [spec:pgorm:req:sql.ast.merge/test]    a kind holds one unconditional arm: the last call wins
#[test]
fn the_last_unconditional_arm_of_a_kind_wins() {
    let sql = merge()
        .when_matched(MatchedAction::Delete)
        .when_matched(MatchedAction::DoNothing)
        .to_string();
    assert_eq!(sql, format!("{} WHEN MATCHED THEN DO NOTHING", head()));
    assert_eq!(arms(&sql), [(Matched, CmdNothing, false)]);
}

// [spec:pgorm:req:sql.render.merge/test]    the matched arms render before the not-matched arms
#[test]
fn matched_arms_render_before_not_matched_arms() {
    let sql = Query::merge(Glyph::Table, Font::Table, on())
        .when_not_matched(NotMatchedAction::InsertDefaultValues)
        .when_matched(MatchedAction::Delete)
        .to_string();
    assert_eq!(
        sql,
        [
            head(),
            r#"WHEN MATCHED THEN DELETE"#,
            r#"WHEN NOT MATCHED THEN INSERT DEFAULT VALUES"#,
        ]
        .join(" ")
    );
    assert_eq!(
        arms(&sql),
        [(Matched, CmdDelete, false), (NotMatched, CmdInsert, false)]
    );
}

// [spec:pgorm:req:sql.render.merge/test]    every action in its grammar position, the
// assignments in the order added
#[test]
fn every_action_renders_where_the_grammar_puts_it() {
    let sql = merge()
        .when_matched_and(
            font(Font::Variant).eq("bold"),
            MergeUpdate::value(Glyph::Image, font(Font::Name))
                .and_value(Glyph::Aspect, Expr::col(Glyph::Aspect).add(1))
                .and_values([(Glyph::Tokens, Expr::val(2).into())]),
        )
        .when_matched_and(font(Font::Variant).eq("gone"), MatchedAction::Delete)
        .when_matched(MatchedAction::DoNothing)
        .when_not_matched_and(
            font(Font::Variant).eq("system"),
            MergeInsert::value(Glyph::Id, font(Font::Id)).overriding(Overriding::SystemValue),
        )
        .when_not_matched_and(
            font(Font::Variant).eq("user"),
            MergeInsert::value(Glyph::Id, font(Font::Id))
                .and_values([(Glyph::Image, font(Font::Name).into())])
                .overriding(Overriding::UserValue),
        )
        .when_not_matched(NotMatchedAction::InsertDefaultValues)
        .to_string();
    assert_eq!(
        sql,
        [
            head(),
            r#"WHEN MATCHED AND "font"."variant" = 'bold' THEN UPDATE SET"#,
            r#""image" = "font"."name", "aspect" = "aspect" + 1, "tokens" = 2"#,
            r#"WHEN MATCHED AND "font"."variant" = 'gone' THEN DELETE"#,
            r#"WHEN MATCHED THEN DO NOTHING"#,
            r#"WHEN NOT MATCHED AND "font"."variant" = 'system' THEN"#,
            r#"INSERT ("id") OVERRIDING SYSTEM VALUE VALUES ("font"."id")"#,
            r#"WHEN NOT MATCHED AND "font"."variant" = 'user' THEN"#,
            r#"INSERT ("id", "image") OVERRIDING USER VALUE VALUES ("font"."id", "font"."name")"#,
            r#"WHEN NOT MATCHED THEN INSERT DEFAULT VALUES"#,
        ]
        .join(" ")
    );
    let clauses = parsed_nodes(&sql, "MergeWhenClause");
    let overriding: Vec<_> = clauses
        .iter()
        .map(|clause| OverridingKind::try_from(code(clause, "override")).expect("a kind"))
        .collect();
    assert_eq!(
        overriding[3..],
        [
            OverridingKind::OverridingSystemValue,
            OverridingKind::OverridingUserValue,
            OverridingKind::OverridingNotSet,
        ]
    );
    let set: Vec<_> = clauses[0]["target_list"]
        .as_array()
        .expect("the update's assignments")
        .iter()
        .map(|target| target["ResTarget"]["name"].clone())
        .collect();
    assert_eq!(set, ["image", "aspect", "tokens"]);
}

// [spec:pgorm:req:sql.ast.merge/test]    the insert's columns and its row come from the same
// pairs, so they always have the same length
// [spec:pgorm:req:sql.render.merge/test]
#[test]
fn an_inserts_columns_and_row_line_up() {
    let sql = merge()
        .when_not_matched(
            MergeInsert::value(Glyph::Id, font(Font::Id))
                .and_value(Glyph::Image, font(Font::Name))
                .and_values([
                    (Glyph::Aspect, Expr::val(1.5).into()),
                    (Glyph::Tokens, Expr::val(3).into()),
                ]),
        )
        .to_string();
    let clauses = parsed_nodes(&sql, "MergeWhenClause");
    let columns: Vec<_> = clauses[0]["target_list"]
        .as_array()
        .expect("the insert's columns")
        .iter()
        .map(|target| target["ResTarget"]["name"].clone())
        .collect();
    assert_eq!(columns, ["id", "image", "aspect", "tokens"]);
    assert_eq!(
        clauses[0]["values"].as_array().map(Vec::len),
        Some(columns.len()),
        "{sql}"
    );
}

// [spec:pgorm:req:sql.ast.merge/test]    the target is a named table: schema, alias and ONLY
// [spec:pgorm:req:sql.render.merge/test]
#[test]
fn target_takes_schema_alias_and_only() {
    let target = (Name::runtime("app"), Glyph::Table)
        .into_named_table()
        .alias(Name::runtime("g"));
    let on = Expr::col((Name::runtime("g"), Glyph::Id)).equals((Font::Table, Font::Id));

    let inherited = Query::merge(target.clone(), Font::Table, on.clone())
        .when_matched(MatchedAction::Delete)
        .to_string();
    assert_eq!(
        inherited,
        r#"MERGE INTO "app"."glyph" AS "g" USING "font" ON "g"."id" = "font"."id" WHEN MATCHED THEN DELETE"#
    );
    let relation = &statement(&inherited)["relation"];
    assert_eq!(relation["schemaname"], "app");
    assert_eq!(relation["relname"], "glyph");
    assert_eq!(relation["alias"]["aliasname"], "g");
    assert_eq!(relation["inh"], true);

    let only = Query::merge(target, Font::Table, on)
        .when_matched(MatchedAction::Delete)
        .only()
        .to_string();
    assert_eq!(
        only,
        r#"MERGE INTO ONLY "app"."glyph" AS "g" USING "font" ON "g"."id" = "font"."id" WHEN MATCHED THEN DELETE"#
    );
    assert_eq!(statement(&only)["relation"]["inh"], false);
}

// [spec:pgorm:req:sql.ast.merge/test]    the source is any relation a FROM clause takes
// [spec:pgorm:req:sql.render.merge/test]
#[test]
fn the_source_is_any_from_item() {
    let s = || Name::runtime("s");
    let on = || Expr::col((Glyph::Table, Glyph::Id)).equals((s(), Font::Id));
    let delete = |source: FromItem| {
        Query::merge(Glyph::Table, source, on())
            .when_matched(MatchedAction::Delete)
            .to_string()
    };

    let subquery = delete(FromItem::SubQuery(
        Query::select().column(Font::Id).from(Font::Table).take(),
        s(),
    ));
    assert_eq!(
        subquery,
        r#"MERGE INTO "glyph" USING (SELECT "id" FROM "font") AS "s" ON "glyph"."id" = "s"."id" WHEN MATCHED THEN DELETE"#
    );
    assert!(
        statement(&subquery)["source_relation"]
            .get("RangeSubselect")
            .is_some()
    );

    let values = delete(FromItem::ValuesList(vec![1i32.into_value_tuple()], s()));
    assert_eq!(
        values,
        r#"MERGE INTO "glyph" USING (VALUES (1)) AS "s" ON "glyph"."id" = "s"."id" WHEN MATCHED THEN DELETE"#
    );
    assert!(
        statement(&values)["source_relation"]
            .get("RangeSubselect")
            .is_some()
    );

    let function = delete(FromItem::FunctionCall(
        Func::named(Name::runtime("generate_series"))
            .args([Expr::val(1).into(), Expr::val(3).into()]),
        s(),
    ));
    assert!(
        statement(&function)["source_relation"]
            .get("RangeFunction")
            .is_some()
    );
}

// [spec:pgorm:def:query.build.with+2/test]    a plain WITH clause prefixes the MERGE
// [spec:pgorm:req:sql.render.merge/test]
#[test]
fn a_with_clause_prefixes_the_merge() {
    let f = || Name::runtime("f");
    let sql = Query::merge(
        Glyph::Table,
        f(),
        Expr::col((Glyph::Table, Glyph::Id)).equals((f(), Font::Id)),
    )
    .when_matched(MatchedAction::Delete)
    .with(
        WithClause::new(CommonTableExpression::new(
            f(),
            Query::select().column(Font::Id).from(Font::Table).take(),
        ))
        .cte(CommonTableExpression::new(
            Name::runtime("g"),
            Query::delete()
                .from_table(Font::Table)
                .returning_all()
                .to_owned(),
        ))
        .to_owned(),
    )
    .to_string();
    assert_eq!(
        sql,
        [
            r#"WITH "f" AS (SELECT "id" FROM "font") , "g" AS (DELETE FROM "font" RETURNING *)"#,
            r#"MERGE INTO "glyph" USING "f" ON "glyph"."id" = "f"."id" WHEN MATCHED THEN DELETE"#,
        ]
        .join(" ")
    );
    let ctes = statement(&sql)["with_clause"]["ctes"]
        .as_array()
        .map(Vec::len);
    assert_eq!(ctes, Some(2));
}

// [spec:pgorm:req:sql.render.merge/test]    every value is bound, numbered in text order
// [spec:pgorm:req:sql.ast.build+3/test]
#[test]
fn build_binds_every_value_in_text_order() {
    let (sql, values) = Query::merge(
        Glyph::Table,
        Font::Table,
        Condition::all()
            .add(on())
            .add(font(Font::Language).eq("en")),
    )
    .when_matched_and(
        Expr::col((Glyph::Table, Glyph::Aspect)).gt(2),
        MergeUpdate::value(Glyph::Image, "rename").and_value(Glyph::Aspect, 3),
    )
    .when_not_matched_and(
        font(Font::Variant).eq("bold"),
        MergeInsert::value(Glyph::Id, font(Font::Id)).and_value(Glyph::Image, "fresh"),
    )
    .build();
    assert_eq!(
        sql,
        [
            r#"MERGE INTO "glyph" USING "font" ON "glyph"."id" = "font"."id" AND "font"."language" = $1"#,
            r#"WHEN MATCHED AND "glyph"."aspect" > $2 THEN UPDATE SET "image" = $3, "aspect" = $4"#,
            r#"WHEN NOT MATCHED AND "font"."variant" = $5 THEN INSERT ("id", "image") VALUES ("font"."id", $6)"#,
        ]
        .join(" ")
    );
    assert_eq!(
        values.0,
        vec![
            "en".into(),
            2i32.into(),
            "rename".into(),
            3i32.into(),
            "bold".into(),
            "fresh".into(),
        ]
    );
}
