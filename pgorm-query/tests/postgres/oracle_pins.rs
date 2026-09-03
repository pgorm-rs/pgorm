//! Renders the PostgreSQL grammar rejects today, and the ones it used to.
//!
//! Each pin asserts the rejection rather than hiding it, so the defect is
//! documented and the pin fails loudly — "oracle pin is stale" — the moment the
//! plan node named in its comment lands and the render becomes valid. Sites that
//! produce these renders are marked with `assert_eq_unparsed!` in the test suite.
//! A pin whose defect is fixed stays here as the positive assertion it became,
//! so the render that was once rejected is held to the grammar instead.

use super::*;
use crate::oracle::{assert_parses, assert_rejected};
use pgorm_query::extension::{Extension, Type};

// Fixed by plan node `bug.oracle-findings`: the frame renderer writes a space
// between the bound value and the keyword, so the offset reads as an offset
// rather than as trailing junk after a numeric literal.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.render.window+3/test]
#[test]
fn window_frame_offset_renders_spaced() {
    let sql = Query::select()
        .from(Char::Table)
        .expr_window(
            Func::count(Expr::col(Char::Id)),
            WindowStatement::partition_by(Char::FontSize)
                .frame_start(FrameType::Rows, Frame::Preceding(2))
                .take(),
        )
        .to_string(QueryBuilder);

    assert!(sql.contains("ROWS 2 PRECEDING"));
    assert_parses(&sql);
}

// Fixed by plan node `unrep.over-function`, at the type level per
// [dec:pgorm:invalid-states-unrepresentable]: the four `expr_window*`
// constructors take a `FunctionCall`, the only production PostgreSQL admits
// `OVER` after, so a windowed column reference no longer typechecks. The
// rejection it used to be pinned to is proved by the `compile_fail` doctest on
// `SelectStatement::expr_window`.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:def:sql.ast.window-statement+2/test]
// [spec:pgorm:req:sql.render.window+3/test]
#[test]
fn over_attaches_only_to_function_calls() {
    let sql = Query::select()
        .from(Char::Table)
        .expr_window(
            Func::count(Expr::col(Char::Id)),
            WindowStatement::partition_by(Char::FontSize),
        )
        .to_string(QueryBuilder);

    assert_eq!(
        sql,
        r#"SELECT COUNT("id") OVER ( PARTITION BY "font_size" ) FROM "character""#
    );
    assert_parses(&sql);
}

// The MySQL-era table options (`ENGINE=`, `COLLATE=`, `DEFAULT CHARSET=`) have
// no PostgreSQL spelling, and `TableCreateStatement` carries no options for them
// to come from: nothing but the caller's own `extra` string can follow the
// closing parenthesis. Deletion-proof, so there is no rejection left to pin.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.create-table+6/test]
#[test]
fn create_table_renders_no_trailing_options() {
    let sql = Table::create(Glyph::Table)
        .col(ColumnDef::new(Glyph::Id).integer())
        .to_string(QueryBuilder);

    assert_eq!(sql, r#"CREATE TABLE "glyph" ( "id" integer )"#);
    assert_parses(&sql);
}

// Fixed by plan node `unrep.on-conflict`, at the type level per
// [dec:pgorm:invalid-states-unrepresentable]: a clause is a target paired with
// an action or a bare `DO NOTHING`, the update assignments are non-empty by
// construction, and only the update carries a filter. The three renders this
// pin held — a target with no action, an empty `DO UPDATE SET`, and a filter on
// `DO NOTHING` — no longer typecheck; the `compile_fail` doctests on
// `OnConflict` prove it. What remains is the shapes the grammar does accept.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.render.on-conflict+1/test]
#[test]
fn on_conflict_renders_only_valid_shapes() {
    let insert = |conflict: OnConflict| {
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect])
            .values_panic([1.into()])
            .on_conflict(conflict)
            .to_string(QueryBuilder)
    };

    let bare = insert(OnConflict::do_nothing());
    let targeted = insert(OnConflict::column(Glyph::Id).do_nothing());
    let updated = insert(
        OnConflict::column(Glyph::Id)
            .and_where(Expr::col(Glyph::Aspect).is_null())
            .update_column(Glyph::Aspect)
            .and_where(Expr::col(Glyph::Image).gt(0))
            .into(),
    );

    assert!(bare.ends_with("ON CONFLICT DO NOTHING"));
    assert!(targeted.ends_with(r#"ON CONFLICT ("id") DO NOTHING"#));
    assert!(updated.ends_with(
        r#"ON CONFLICT ("id") WHERE "aspect" IS NULL DO UPDATE SET "aspect" = "excluded"."aspect" WHERE "image" > 0"#
    ));
    assert_parses(&bare);
    assert_parses(&targeted);
    assert_parses(&updated);
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

// Fixed by plan node `bug.oracle-findings`, at the type level per
// [dec:pgorm:invalid-states-unrepresentable]: the constraint travels inside
// `JoinKind`, so a cross join carries none to render and `JoinType` no longer
// spells one.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.render.joins+2/test]
// [spec:pgorm:req:sql.ast.select.join+1/test]
#[test]
fn cross_join_renders_without_on_clause() {
    let sql = Query::select()
        .column(Char::Id)
        .from(Char::Table)
        .cross_join(Font::Table)
        .to_string(QueryBuilder);

    assert_eq!(sql, r#"SELECT "id" FROM "character" CROSS JOIN "font""#);
    assert_parses(&sql);
}

// Fixed by plan node `bug.oracle-findings`, at the type level per
// [dec:pgorm:invalid-states-unrepresentable]: PostgreSQL admits RENAME only as
// the sole action of an ALTER TABLE, so a column rename is a statement of its
// own and cannot be listed beside an ADD COLUMN.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.alter-table+3/test]
#[test]
fn column_rename_is_its_own_statement() {
    let added = Table::alter(Font::Table)
        .add_column(ColumnDef::new(Alias::new("new_col")).integer())
        .to_string(QueryBuilder);
    let renamed = Table::rename_column(Font::Table, Font::Name, Alias::new("name_new"))
        .to_string(QueryBuilder);

    assert_eq!(
        renamed,
        r#"ALTER TABLE "font" RENAME COLUMN "name" TO "name_new""#
    );
    assert_parses(&added);
    assert_parses(&renamed);
}

// Fixed by plan node `bug.oracle-findings`, at the type level per
// [dec:pgorm:invalid-states-unrepresentable]: the target is a `DynIden` rather
// than a `TableName`, so the qualified form a rename cannot honour — the table
// stays in the schema it is already in — no longer typechecks.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.drop-rename-truncate+3/test]
#[test]
fn table_rename_target_is_bare_name() {
    let sql = Table::rename((Alias::new("schema"), Font::Table), Alias::new("font_new"))
        .to_string(QueryBuilder);

    assert_eq!(sql, r#"ALTER TABLE "schema"."font" RENAME TO "font_new""#);
    assert_parses(&sql);
}

// Fixed by plan node `bug.oracle-findings`: the `RENAME TO` target is a type
// name, not an enum label, so it leaves the value pipeline and renders as the
// quoted identifier the grammar wants.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.type-alter-drop+2/test]
#[test]
fn alter_type_rename_emits_identifier() {
    let sql = Type::alter()
        .name(Font::Table)
        .rename_to(Alias::new("typeface"))
        .to_string(QueryBuilder);

    assert_eq!(sql, r#"ALTER TYPE "font" RENAME TO "typeface""#);
    assert_parses(&sql);
}

// Fixed by plan node `bug.oracle-findings`, at the type level per
// [dec:pgorm:invalid-states-unrepresentable]: PostgreSQL takes a precision only
// where the trailing field is SECOND, so the precision rides on the
// second-bearing fields and `interval HOUR(43)` has no spelling to render.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:def:sql.render.ddl.types+3/test]
// [spec:pgorm:def:sql.types.column-type+3/test]
#[test]
fn interval_precision_rides_on_seconds() {
    let hour = Table::create(Glyph::Table)
        .col(ColumnDef::new(Glyph::Aspect).interval(IntervalSpec::Fields(PgInterval::Hour)))
        .to_string(QueryBuilder);
    let seconds = Table::create(Glyph::Table)
        .col(ColumnDef::new(Glyph::Aspect).interval(IntervalSpec::Fields(
            PgInterval::HourToSecond(Some(IntervalPrecision::P3)),
        )))
        .to_string(QueryBuilder);

    assert!(hour.contains("interval HOUR"));
    assert!(seconds.contains("interval HOUR TO SECOND(3)"));
    assert_eq!(IntervalPrecision::new(43), None);
    assert_parses(&hour);
    assert_parses(&seconds);
}

// Fixed by plan node `bug.oracle-findings`, at the type level per
// [dec:pgorm:invalid-states-unrepresentable]: PostgreSQL takes at most one drop
// behaviour, so the two spellings share one slot and the later call wins.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.extension+2/test]
#[test]
fn extension_drop_takes_one_behaviour() {
    let sql = Extension::drop()
        .name("ltree")
        .cascade()
        .restrict()
        .to_string(QueryBuilder);

    assert_eq!(sql, r#"DROP EXTENSION "ltree" RESTRICT"#);
    assert_parses(&sql);
}

// Half fixed by plan node `bug.oracle-findings`: the extension name and schema
// route through identifier quoting and the version through the string-literal
// escape, so neither an odd version nor an embedded quote escapes the grammar.
// `ColumnDef::extra` stays raw by design — it is the escape hatch for column SQL
// the type vocabulary cannot spell, documented as caller responsibility by
// `sql.ddl.column-def`, so it keeps its pin.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:sem:sql.render.ddl.extension+1/test]
// [spec:pgorm:req:sql.ddl.column-def+3/test]
#[test]
fn oracle_pins_extra_interpolated_raw() {
    let version = Extension::create()
        .name("ltree")
        .version("v0.1.0")
        .to_string(QueryBuilder);
    let injected = Extension::create()
        .name(r#"pg"weird ext"#)
        .to_string(QueryBuilder);
    let extra = Table::create(Glyph::Table)
        .col(
            ColumnDef::new(Glyph::Id)
                .integer()
                .extra("ANYTHING I WANT TO SAY".to_owned()),
        )
        .to_string(QueryBuilder);

    assert_eq!(version, r#"CREATE EXTENSION "ltree" VERSION 'v0.1.0'"#);
    assert_eq!(injected, r#"CREATE EXTENSION "pg""weird ext""#);
    assert_parses(&version);
    assert_parses(&injected);
    assert!(assert_rejected(&extra).contains("ANYTHING"));
}

// Fixed by plan node `bug.oracle-findings`, at the type level per
// [dec:pgorm:invalid-states-unrepresentable]: `NullAlias` existed only as a
// placeholder inside `ColumnDef::take`, which now clones the name, so the empty
// identifier PostgreSQL rejects has no constructor left.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.render.ident-quoting/test]
// [spec:pgorm:def:sql.ast.keywords+2/test]
#[test]
fn alias_identifiers_are_never_empty() {
    let sql = Query::select()
        .expr_as(Expr::col(Glyph::Aspect), Alias::new("ratio"))
        .from(Glyph::Table)
        .to_string(QueryBuilder);
    let taken = ColumnDef::new(Glyph::Id).integer().take();

    assert_eq!(sql, r#"SELECT "aspect" AS "ratio" FROM "glyph""#);
    assert_eq!(taken.get_column_name(), "id");
    assert_parses(&sql);
}

// Fixed by plan node `bug.oracle-findings`, at the type level per
// [dec:pgorm:invalid-states-unrepresentable]: ESCAPE left the operator lexicon
// for `SimpleExpr::LikePattern`, the one place the grammar admits it, so it can
// no longer be applied to two arbitrary operands.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:def:sql.render.operators+1/test]
// [spec:pgorm:def:sql.types.opers+1/test]
#[test]
fn escape_renders_only_inside_like() {
    let sql = Query::select()
        .expr(Expr::col(Glyph::Image).like(LikeExpr::new("a%").escape('\\')))
        .to_string(QueryBuilder);

    assert!(sql.contains(r#"LIKE 'a%' ESCAPE E'\\'"#));
    assert_parses(&sql);
}

// The oracle's ceiling, recorded rather than pinned: this render is grammatical,
// so no parser can catch it. An empty select list is valid PostgreSQL and is
// closed as an ORM-layer error by `sql.empty-select-list`. The other members of
// this family are closed by the type system instead: `money(12, 2)` — which
// only a resolved type modifier rejects — is gone with `ColumnType::Money`'s
// precision argument, a cross-database reference is gone with the
// database-qualified `TableName` form, and `ON CONFLICT DO UPDATE` with no
// inference specification — which parse analysis rejects with "requires
// inference specification or constraint name" — is gone with `OnConflict`'s
// targeted variant.
// [spec:pgorm:req:sql.render.oracle/test]
#[test]
fn oracle_records_parse_valid_defects() {
    let empty_select_list = Query::select().from(Glyph::Table).to_string(QueryBuilder);

    assert_eq!(empty_select_list, r#"SELECT  FROM "glyph""#);
    assert_parses(&empty_select_list);
    assert_parses(
        r#"INSERT INTO "glyph" ("aspect") VALUES (1) ON CONFLICT DO UPDATE SET "aspect" = 1"#,
    );
}

// Fixed by plan node `unrep.ddl-empty-builders`, at the type level per
// [dec:pgorm:invalid-states-unrepresentable]: PostgreSQL parses neither an
// action-less `ALTER TABLE` nor an empty index column list, so `Table::alter`
// yields a `PendingTableAlter` that only an action turns into a statement, and
// `Index::create` takes the first column. The `No alter option found` panic went
// with the state it guarded — the strings below have no builder left to produce
// them.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.alter-table+3/test]
// [spec:pgorm:req:sql.ddl.index-create+4/test]
// [spec:pgorm:sem:sql.ddl.panics+4/test]
#[test]
fn empty_ddl_collections_do_not_construct() {
    let altered = Table::alter(Font::Table)
        .drop_column(Font::Name)
        .to_string(QueryBuilder);
    let indexed = Index::create(Glyph::Table, Glyph::Aspect)
        .name("idx")
        .to_string(QueryBuilder);

    assert_eq!(altered, r#"ALTER TABLE "font" DROP COLUMN "name""#);
    assert_eq!(indexed, r#"CREATE INDEX "idx" ON "glyph" ("aspect")"#);
    assert_parses(&altered);
    assert_parses(&indexed);
    assert!(assert_rejected(r#"ALTER TABLE "font""#).contains("end of input"));
    assert_rejected(r#"CREATE INDEX "idx" ON "glyph" ()"#);
    assert_rejected(r#"CREATE TABLE "glyph" ( "id" integer, PRIMARY KEY () )"#);
}

// The oracle's verdict on the third member of the empty-builder family, recorded
// rather than designed out: `CREATE TABLE t ()` is valid PostgreSQL — a table
// with no attributes is a real table — so a column-less create statement stays
// buildable and is documented by `sql.ddl.create-table` instead.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.create-table+6/test]
#[test]
fn create_table_with_no_columns_is_valid() {
    let sql = Table::create(Glyph::Table).to_string(QueryBuilder);

    assert_eq!(sql, r#"CREATE TABLE "glyph" (  )"#);
    assert_parses(&sql);
}

// Fixed by plan node `unrep.ddl-table-target`, at the type level per
// [dec:pgorm:invalid-states-unrepresentable]: every DDL statement whose render
// names a target takes that target in its constructor, so the absent name
// PostgreSQL rejects at the token after it has nowhere to come from. The
// `compile_fail` doctests on each statement type prove the constructors refuse.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.create-table+6/test]
// [spec:pgorm:req:sql.ddl.index-create+4/test]
// [spec:pgorm:req:sql.ddl.index-drop+2/test]
// [spec:pgorm:req:sql.ddl.drop-rename-truncate+3/test]
// [spec:pgorm:req:sql.ddl.alter-table+3/test]
// [spec:pgorm:req:sql.ddl.foreign-key+2/test]
#[test]
fn ddl_targets_are_taken_by_construction() {
    let rendered = [
        Table::create(Glyph::Table)
            .col(ColumnDef::new(Glyph::Id).integer())
            .to_string(QueryBuilder),
        Table::drop(Glyph::Table).to_string(QueryBuilder),
        Table::truncate(Glyph::Table).to_string(QueryBuilder),
        Table::rename(Glyph::Table, Alias::new("g")).to_string(QueryBuilder),
        Table::rename_column(Glyph::Table, Glyph::Id, Alias::new("gid")).to_string(QueryBuilder),
        Index::create(Glyph::Table, Glyph::Aspect)
            .name("idx")
            .to_string(QueryBuilder),
        Index::drop("idx").to_string(QueryBuilder),
        ForeignKey::drop(Char::Table, "fk").to_string(QueryBuilder),
    ];

    for sql in &rendered {
        assert_parses(sql);
    }
    assert_rejected(r#"CREATE TABLE  ( "id" integer )"#);
    assert_rejected("DROP TABLE ");
    assert_rejected("TRUNCATE TABLE ");
    assert_rejected(r#"ALTER TABLE  RENAME TO "g""#);
    assert_rejected(r#"ALTER TABLE "glyph" RENAME COLUMN  TO "gid""#);
    assert_rejected(r#"CREATE INDEX "idx" ON  ("aspect")"#);
    assert_rejected("DROP INDEX ");
    assert_rejected(r#"ALTER TABLE  DROP CONSTRAINT "fk""#);
}

// The two targets the oracle says PostgreSQL does not need, recorded rather than
// required: an index name is derived by the server when `CREATE INDEX` omits it,
// and `DROP INDEX` names a schema-scoped index rather than a table, so both stay
// optional where the rest of the family moved into the constructor.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.index-create+4/test]
// [spec:pgorm:req:sql.ddl.index-drop+2/test]
#[test]
fn index_name_and_drop_table_stay_optional() {
    let unnamed = Index::create(Glyph::Table, Glyph::Aspect).to_string(QueryBuilder);
    let untabled = Index::drop("idx").to_string(QueryBuilder);

    assert_eq!(unnamed, r#"CREATE INDEX  ON "glyph" ("aspect")"#);
    assert_eq!(untabled, r#"DROP INDEX "idx""#);
    assert_parses(&unnamed);
    assert_parses(&untabled);
}

// No plan node yet, and the residual of `unrep.ddl-table-target`: the foreign-key
// and type/extension builders still carry their targets as fields a caller may
// never fill. A foreign key's own table is rendered only standalone, and its two
// column lists are the empty-collection axis rather than this one, so closing it
// is a redesign of `TableForeignKey` rather than a constructor argument; the type
// and extension statements name a type rather than a table, and `AS ENUM` drops
// the parentheses the grammar wants when the value list is empty.
// [spec:pgorm:req:sql.render.oracle/test]
// [spec:pgorm:req:sql.ddl.foreign-key+2/test]
// [spec:pgorm:req:sql.ddl.type-enum+1/test]
// [spec:pgorm:req:sql.ddl.type-alter-drop+2/test]
// [spec:pgorm:req:sql.ddl.extension+2/test]
#[test]
fn oracle_pins_ddl_targets_left_open() {
    let no_ref_table = ForeignKey::create()
        .name("fk")
        .from(Char::Table, Char::FontId)
        .to_string(QueryBuilder);
    let no_from_table = ForeignKey::create()
        .to(Font::Table, Font::Id)
        .to_string(QueryBuilder);
    let no_type_name = Type::create().to_string(QueryBuilder);
    let no_enum_values = Type::create()
        .as_enum(Alias::new("font_family"))
        .to_string(QueryBuilder);
    let no_extension_name = Extension::create().to_string(QueryBuilder);

    assert!(no_ref_table.ends_with("REFERENCES  ()"));
    assert!(no_from_table.starts_with("ALTER TABLE  ADD FOREIGN KEY ()"));
    assert_eq!(no_type_name, "CREATE TYPE ");
    assert_eq!(no_enum_values, r#"CREATE TYPE "font_family" AS ENUM"#);
    assert_eq!(no_extension_name, r#"CREATE EXTENSION """#);
    assert_rejected(&no_ref_table);
    assert_rejected(&no_from_table);
    assert_rejected(&no_type_name);
    assert_rejected(&no_enum_values);
    assert_rejected(&no_extension_name);
    assert_rejected(&Type::drop().to_string(QueryBuilder));
    assert_rejected(&Type::alter().to_string(QueryBuilder));
    assert_rejected(&Extension::drop().to_string(QueryBuilder));
}
