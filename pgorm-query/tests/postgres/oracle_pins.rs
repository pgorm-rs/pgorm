//! Renders the PostgreSQL grammar rejects today.
//!
//! Each pin asserts the rejection rather than hiding it, so the defect is
//! documented and the pin fails loudly — "oracle pin is stale" — the moment the
//! plan node named in its comment lands and the render becomes valid. Sites that
//! produce these renders are marked with `assert_eq_unparsed!` in the test suite.

use super::*;
use crate::oracle::{assert_parses, assert_rejected};
use pgorm_query::extension::{Extension, Type};

// No plan node yet. `sql.render.window` pins the missing space as a known
// limitation of the frame renderer: the bound value is written immediately
// against the keyword, so PostgreSQL lexes `2PRECEDING` as trailing junk after a
// numeric literal.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.render.window+1/test]
#[test]
fn oracle_pins_window_frame_offset_spacing() {
    let sql = Query::select()
        .from(Char::Table)
        .expr_window(
            Func::count(Expr::col(Char::Id)),
            WindowStatement::partition_by(Char::FontSize)
                .frame_start(FrameType::Rows, Frame::Preceding(2))
                .take(),
        )
        .to_string(QueryBuilder);

    assert!(sql.contains("ROWS 2PRECEDING"));
    assert!(assert_rejected(&sql).contains("2PRECEDING"));
    assert_parses(&sql.replace("2PRECEDING", "2 PRECEDING"));
}

// Follow-up: narrow the four `expr_window*` constructors from `Into<SimpleExpr>`
// to `FunctionCall`, per [dec:pgorm:invalid-states-unrepresentable]. No plan node
// yet — it is an API break, not a render fix. PostgreSQL allows OVER only after a
// function call, so a windowed column reference stays constructible in the AST
// with no valid rendering, and no change to the renderer can close it.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:def:sql.ast.window-statement+1/test]
#[test]
fn oracle_pins_over_on_a_bare_expression() {
    let sql = Query::select()
        .from(Char::Table)
        .expr_window(
            Expr::col(Char::Character),
            WindowStatement::partition_by(Char::FontSize),
        )
        .to_string(QueryBuilder);

    assert!(sql.starts_with(r#"SELECT "character" OVER ("#));
    assert!(assert_rejected(&sql).contains("OVER"));
}

// Fixed by plan node `unrep.mysql-purge`: `TableOpt::Engine`, `Collate` and
// `CharacterSet` are MySQL-era table options with no PostgreSQL spelling.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.create-table+3/test]
#[test]
fn oracle_pins_mysql_table_options() {
    let table = || {
        Table::create()
            .table(Glyph::Table)
            .col(ColumnDef::new(Glyph::Id).integer())
            .to_owned()
    };

    for (sql, keyword) in [
        (table().engine("InnoDB").to_string(QueryBuilder), "ENGINE"),
        (
            table()
                .collate("utf8mb4_unicode_ci")
                .to_string(QueryBuilder),
            "COLLATE",
        ),
        (
            table().character_set("utf8mb4").to_string(QueryBuilder),
            "DEFAULT",
        ),
    ] {
        assert!(assert_rejected(&sql).contains(keyword), "{sql}");
    }
}

// Fixed by plan node `unrep.on-conflict`: targets, action and action-filter are
// three independent fields, so a target without an action, an empty update set,
// and a filter on DO NOTHING are all constructible.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.render.on-conflict/test]
#[test]
fn oracle_pins_on_conflict_shapes() {
    let insert = |conflict: OnConflict| {
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect])
            .values_panic([1.into()])
            .on_conflict(conflict)
            .to_string(QueryBuilder)
    };

    let no_action = insert(OnConflict::column(Glyph::Id).to_owned());
    let empty_set = insert(
        OnConflict::column(Glyph::Id)
            .update_columns::<Glyph, _>([])
            .to_owned(),
    );
    let filtered_nothing = insert(
        OnConflict::column(Glyph::Id)
            .do_nothing()
            .action_and_where(Expr::col(Glyph::Aspect).gt(0))
            .to_owned(),
    );

    assert!(no_action.ends_with(r#"ON CONFLICT ("id")"#));
    assert!(empty_set.ends_with("DO UPDATE SET "));
    assert!(filtered_nothing.contains(r#"DO NOTHING WHERE "aspect" > 0"#));
    assert_rejected(&no_action);
    assert_rejected(&empty_set);
    assert!(assert_rejected(&filtered_nothing).contains("WHERE"));
}

// No plan node yet. `sql.render.update-delete` pins the behaviour: ORDER BY and
// LIMIT are rendered on UPDATE and DELETE when populated, and PostgreSQL accepts
// neither.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.render.update-delete/test]
#[test]
fn oracle_pins_update_delete_order_limit() {
    let update = Query::update()
        .table(Glyph::Table)
        .value(Glyph::Aspect, 1)
        .order_by(Glyph::Id, Order::Asc)
        .limit(1)
        .to_string(QueryBuilder);
    let delete = Query::delete()
        .from_table(Glyph::Table)
        .order_by(Glyph::Id, Order::Asc)
        .limit(1)
        .to_string(QueryBuilder);

    assert!(assert_rejected(&update).contains("ORDER"));
    assert!(assert_rejected(&delete).contains("ORDER"));
}

// No plan node yet. Every join carries a constraint through the same
// `prepare_join_on` hook, and an empty condition renders `ON TRUE` rather than
// nothing, so a CROSS JOIN always gains an ON clause PostgreSQL forbids.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.render.joins/test]
#[test]
fn oracle_pins_cross_join_on_clause() {
    let sql = Query::select()
        .column(Char::Id)
        .from(Char::Table)
        .join(JoinType::CrossJoin, Font::Table, Condition::all())
        .to_string(QueryBuilder);

    assert!(sql.contains(r#"CROSS JOIN "font" ON TRUE"#));
    assert!(assert_rejected(&sql).contains("ON"));
}

// No plan node yet. ALTER TABLE options are joined with commas, but PostgreSQL
// admits RENAME only as the sole action of the statement.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.alter-table/test]
#[test]
fn oracle_pins_alter_table_multi_action() {
    let sql = Table::alter()
        .table(Font::Table)
        .add_column(ColumnDef::new(Alias::new("new_col")).integer())
        .rename_column(Font::Name, Alias::new("name_new"))
        .to_string(QueryBuilder);

    assert!(sql.contains(", RENAME COLUMN"));
    assert!(assert_rejected(&sql).contains("RENAME"));
}

// No plan node yet. The rename target is rendered through the same qualified
// table reference as the source, but PostgreSQL's RENAME TO takes a bare name —
// the new table stays in the schema it is already in.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.drop-rename-truncate+1/test]
#[test]
fn oracle_pins_table_rename_qualified_target() {
    let sql = Table::rename()
        .table(
            (Alias::new("schema"), Font::Table),
            (Alias::new("schema"), Alias::new("font_new")),
        )
        .to_string(QueryBuilder);

    assert!(sql.ends_with(r#"RENAME TO "schema"."font_new""#));
    assert_rejected(&sql);
    assert_parses(&sql.replace(r#"TO "schema"."font_new""#, r#"TO "font_new""#));
}

// No plan node yet. `ALTER TYPE … RENAME TO` routes its operand through the enum
// label path, so the new type name is emitted as a string literal where the
// grammar wants an identifier.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.type-alter-drop/test]
#[test]
fn oracle_pins_alter_type_rename_literal() {
    let sql = Type::alter()
        .name(Font::Table)
        .rename_to(Alias::new("typeface"))
        .to_string(QueryBuilder);

    assert!(sql.ends_with("RENAME TO 'typeface'"));
    assert!(assert_rejected(&sql).contains("'typeface'"));
    assert_parses(&sql.replace("'typeface'", r#""typeface""#));
}

// Fixed by plan node `unrep.mysql-purge` in spirit: the interval renderer appends
// a precision to the field spelling, but PostgreSQL takes a precision only on the
// trailing second, as `interval hour to second(p)`.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:def:sql.render.ddl.types+1/test]
#[test]
fn oracle_pins_interval_field_precision() {
    let sql = Table::create()
        .table(Glyph::Table)
        .col(ColumnDef::new(Glyph::Aspect).interval(Some(PgInterval::Hour), Some(43)))
        .to_string(QueryBuilder);

    assert!(sql.contains("interval HOUR(43)"));
    assert_rejected(&sql);
    assert_parses(&sql.replace("HOUR(43)", "HOUR"));
}

// No plan node yet. CASCADE and RESTRICT are independent flags on DROP EXTENSION
// and both are emitted when both are set; PostgreSQL takes at most one.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.extension/test]
#[test]
fn oracle_pins_extension_cascade_restrict() {
    let sql = Extension::drop()
        .name("ltree")
        .cascade()
        .restrict()
        .to_string(QueryBuilder);

    assert!(sql.ends_with("CASCADE RESTRICT"));
    assert!(assert_rejected(&sql).contains("RESTRICT"));
}

// Caller responsibility, pinned by `sql.render.ddl.extension`: extension name,
// schema and version, and `ColumnDef::extra`, are interpolated raw. Untrusted or
// merely unlucky strings render SQL no grammar accepts.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:sem:sql.render.ddl.extension/test]
#[test]
fn oracle_pins_raw_interpolated_strings() {
    let version = Extension::create()
        .name("ltree")
        .version("v0.1.0")
        .to_string(QueryBuilder);
    let injected = Extension::create()
        .name(r#"pg"weird ext"#)
        .to_string(QueryBuilder);
    let extra = Table::create()
        .table(Glyph::Table)
        .col(
            ColumnDef::new(Glyph::Id)
                .integer()
                .extra("ANYTHING I WANT TO SAY".to_owned()),
        )
        .to_string(QueryBuilder);

    assert_rejected(&version);
    assert_rejected(&injected);
    assert!(assert_rejected(&extra).contains("ANYTHING"));
}

// No plan node yet. `NullAlias` renders an empty quoted identifier, and PostgreSQL
// rejects a zero-length delimited identifier.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.render.ident-quoting/test]
#[test]
fn oracle_pins_empty_identifier_alias() {
    let sql = Query::select()
        .expr_as(Expr::col(Glyph::Aspect), NullAlias::new())
        .from(Glyph::Table)
        .to_string(QueryBuilder);

    assert!(sql.contains(r#"AS """#));
    assert_rejected(&sql);
}

// No plan node yet. `BinOper::Escape` is a free-standing operator in the lexicon,
// but ESCAPE is grammatical only as the tail of a LIKE / ILIKE pattern.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:def:sql.render.operators/test]
#[test]
fn oracle_pins_escape_outside_like() {
    let sql = Query::select()
        .expr(Expr::col(Glyph::Aspect).binary(BinOper::Escape, Expr::val(1)))
        .to_string(QueryBuilder);

    assert!(sql.contains("ESCAPE"));
    assert_rejected(&sql);
    assert_parses(
        &Query::select()
            .expr(Expr::col(Glyph::Image).like(LikeExpr::new("a%").escape('\\')))
            .to_string(QueryBuilder),
    );
}

// The oracle's ceiling, recorded rather than pinned: these renders are
// grammatical, so no parser can catch them. `money(12, 2)` is rejected only when
// the type modifier is resolved (`unrep.mysql-purge` deletes `money_len`); an
// empty select list is valid PostgreSQL and was closed as an ORM-layer error by
// `sql.empty-select-list`. The third member of this family, a cross-database
// reference, is closed by the type system instead: `TableName` has no
// database-qualified form to render.
// [spec:pgorm:req:sql.render.oracle/test]
#[test]
fn oracle_records_parse_valid_defects() {
    let money = Table::create()
        .table(Glyph::Table)
        .col(ColumnDef::new(Glyph::Aspect).money_len(12, 2))
        .to_string(QueryBuilder);
    let empty_select_list = Query::select().from(Glyph::Table).to_string(QueryBuilder);

    assert!(money.contains("money(12, 2)"));
    assert_eq!(empty_select_list, r#"SELECT  FROM "glyph""#);

    assert_parses(&money);
    assert_parses(&empty_select_list);
}
