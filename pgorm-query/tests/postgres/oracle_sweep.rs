use super::*;
use crate::oracle::{assert_parses, assert_query_eq};
use pgorm_query::extension::{Extension, Type};

fn sweep(statements: impl IntoIterator<Item = String>) {
    for sql in statements {
        assert_parses(&sql);
    }
}

fn base() -> SelectStatement {
    Query::select().column(Glyph::Id).from(Glyph::Table).take()
}

// [spec:pgorm:req:sql.render.oracle/test]    the select clause vocabulary
// [spec:pgorm:req:sql.render.select-order+1/test]
#[test]
fn sweep_select_clause_shapes() {
    sweep([
        base().to_string(),
        Query::select()
            .column(Asterisk)
            .from(Glyph::Table)
            .to_string(),
        Query::select()
            .distinct()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .to_string(),
        Query::select()
            .distinct_on([Glyph::Aspect])
            .column(Glyph::Id)
            .from(Glyph::Table)
            .to_string(),
        Query::select()
            .expr_as(Expr::col(Glyph::Id), Alias::new("glyph id"))
            .from((Alias::new("public"), Glyph::Table))
            .to_string(),
        base()
            .and_where(Expr::col(Glyph::Aspect).gt(1))
            .and_where(Expr::col(Glyph::Image).is_not_null())
            .add_group_by([Expr::col(Glyph::Id).into()])
            .and_having(Expr::col(Glyph::Aspect).lt(9))
            .to_string(),
        base()
            .order_by(Glyph::Id, Order::Desc)
            .order_by_with_nulls(Glyph::Aspect, Order::Asc, NullOrdering::First)
            .limit(10)
            .offset(5)
            .to_string(),
        base()
            .order_by_with_nulls(
                Glyph::Id,
                Order::Field(Values(vec![1.into(), 2.into()])),
                NullOrdering::Last,
            )
            .to_string(),
        Query::select()
            .column((Glyph::Table, Glyph::Id))
            .column((Alias::new("public"), Glyph::Table, Glyph::Aspect))
            .from(Glyph::Table)
            .to_string(),
    ]);
}

// [spec:pgorm:req:sql.render.oracle/test]    expression rendering, including the parenthesis
// elision of `sql.render.precedence`
// [spec:pgorm:req:sql.render.parens/test]
#[test]
fn sweep_expression_shapes() {
    let exprs: Vec<SimpleExpr> = vec![
        Expr::col(Glyph::Aspect).add(1).mul(2),
        Expr::col(Glyph::Aspect)
            .gt(1)
            .and(Expr::col(Glyph::Id).lt(9))
            .or(Expr::col(Glyph::Id).eq(3)),
        Expr::col(Glyph::Aspect).between(1, 9),
        Expr::col(Glyph::Aspect).not_between(1, 9),
        Expr::col(Glyph::Image).like("a%"),
        Expr::col(Glyph::Image).like(LikeExpr::new("a%").escape('\\')),
        Expr::col(Glyph::Id).is_in([1, 2, 3]),
        Expr::col(Glyph::Id).is_in::<i32, _>([]),
        Expr::col(Glyph::Id).is_not_in([1, 2]),
        Expr::col(Glyph::Aspect).cast_as(Alias::new("text")),
        Expr::cust("now()"),
        Expr::cust_with_values("$1 + $2", [1, 2]),
        Expr::tuple([Expr::val(1).into(), Expr::val(2).into()]).into(),
        Expr::col(Glyph::Tokens).get_json_field("a"),
        Expr::col(Glyph::Tokens).cast_json_field("b"),
        Expr::col(Glyph::Image).concat(Expr::val("x")),
        SimpleExpr::Unary(UnOper::Not, Box::new(Expr::col(Glyph::Id).eq(1))),
        Expr::exists(base()),
        Expr::col(Glyph::Id).eq(Expr::any(base())),
        CaseStatement::new()
            .case(Expr::col(Glyph::Aspect).gt(1), Expr::val("big"))
            .finally(Expr::val("small"))
            .into(),
        Func::count(Expr::col(Glyph::Id)).into(),
        Func::coalesce([Expr::col(Glyph::Aspect).into(), Expr::val(0).into()]).into(),
        Func::cast_as(Expr::val("1"), Alias::new("int4")).into(),
    ];

    sweep(
        exprs
            .into_iter()
            .map(|expr| Query::select().expr(expr).from(Glyph::Table).to_string()),
    );
}

// [spec:pgorm:req:sql.render.oracle/test]    the join vocabulary
// [spec:pgorm:req:sql.render.joins+2/test]
#[test]
fn sweep_join_shapes() {
    let joins = [
        JoinType::Join,
        JoinType::InnerJoin,
        JoinType::LeftJoin,
        JoinType::RightJoin,
        JoinType::FullOuterJoin,
    ];

    let mut statements: Vec<String> = joins
        .into_iter()
        .map(|join| {
            Query::select()
                .column((Char::Table, Char::Id))
                .from(Char::Table)
                .join(
                    join,
                    Font::Table,
                    Expr::col((Char::Table, Char::FontId)).equals((Font::Table, Font::Id)),
                )
                .to_string()
        })
        .collect();

    statements.push(
        Query::select()
            .column(Char::Id)
            .from(Char::Table)
            .cross_join(Font::Table)
            .to_string(),
    );
    statements.push(
        Query::select()
            .column(Char::Id)
            .from(Char::Table)
            .join_as(
                JoinType::LeftJoin,
                Font::Table,
                Alias::new("f"),
                Expr::col((Char::Table, Char::FontId)).equals((Alias::new("f"), Font::Id)),
            )
            .to_string(),
    );
    statements.push(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .join_lateral(
                JoinType::LeftJoin,
                base(),
                Alias::new("sub"),
                Expr::val(1).eq(1),
            )
            .to_string(),
    );
    statements.push(
        Query::select()
            .column(Glyph::Id)
            .from_subquery(base(), Alias::new("sub"))
            .to_string(),
    );
    statements.push(
        Query::select()
            .column(Alias::new("column1"))
            .from_values([(1, "a"), (2, "b")], Alias::new("v"))
            .to_string(),
    );
    statements.push(
        Query::select()
            .column(Asterisk)
            .from_function(
                Func::cust(Alias::new("generate_series")).arg(1),
                Alias::new("g"),
            )
            .to_string(),
    );

    sweep(statements);
}

// [spec:pgorm:req:sql.render.oracle/test]    set operations and row locking
// [spec:pgorm:sem:sql.render.locking/test]
#[test]
fn sweep_union_and_locking_shapes() {
    let unions = [
        UnionType::Distinct,
        UnionType::All,
        UnionType::Intersect,
        UnionType::Except,
    ];

    let mut statements: Vec<String> = unions
        .into_iter()
        .map(|union| base().union(union, base()).to_string())
        .collect();

    let locks = [
        LockType::Update,
        LockType::NoKeyUpdate,
        LockType::Share,
        LockType::KeyShare,
    ];
    statements.extend(locks.into_iter().map(|lock| base().lock(lock).to_string()));

    statements.push(
        base()
            .lock_with_tables(LockType::Update, [Glyph::Table])
            .to_string(),
    );
    statements.push(
        base()
            .lock_with_behavior(LockType::Update, LockBehavior::Nowait)
            .to_string(),
    );
    statements.push(
        base()
            .lock_with_tables_behavior(LockType::Share, [Glyph::Table], LockBehavior::SkipLocked)
            .to_string(),
    );

    sweep(statements);
}

// [spec:pgorm:req:sql.render.oracle/test]    common table expressions
// [spec:pgorm:req:sql.render.cte+1/test]
#[test]
fn sweep_cte_shapes() {
    let named = |name: &str| {
        CommonTableExpression::new(Alias::new(name), base())
            .column(Glyph::Id)
            .to_owned()
    };
    let cte = || named("cte");
    let outer = || {
        Query::select()
            .column(Glyph::Id)
            .from(Alias::new("cte"))
            .take()
    };

    sweep([
        outer().with(WithClause::new(cte())).to_string(),
        outer()
            .with(WithClause::new(cte().materialized(true).to_owned()))
            .to_string(),
        outer()
            .with(WithClause::new(cte().materialized(false).to_owned()))
            .to_string(),
        outer()
            .with(WithClause::new(cte()).cte(named("other")).to_owned())
            .to_string(),
        outer().with(RecursiveWithClause::new(cte())).to_string(),
    ]);
}

// [spec:pgorm:req:sql.render.oracle/test]    window functions over a real function call
// [spec:pgorm:req:sql.render.window+3/test]
#[test]
fn sweep_window_function_shapes() {
    let over = |window: WindowStatement| {
        Query::select()
            .from(Char::Table)
            .expr_window(Func::count(Expr::col(Char::Id)), window)
            .to_string()
    };

    sweep([
        over(WindowStatement::new()),
        over(WindowStatement::partition_by(Char::FontSize)),
        over(
            WindowStatement::partition_by(Char::FontSize)
                .order_by(Char::Id, Order::Asc)
                .take(),
        ),
        over(
            WindowStatement::partition_by(Char::FontSize)
                .frame_start(FrameType::Rows, Frame::UnboundedPreceding)
                .take(),
        ),
        over(
            WindowStatement::partition_by(Char::FontSize)
                .frame_start(FrameType::Range, Frame::CurrentRow)
                .take(),
        ),
        over(
            WindowStatement::partition_by(Char::FontSize)
                .frame_between(
                    FrameType::Rows,
                    Frame::UnboundedPreceding,
                    Frame::UnboundedFollowing,
                )
                .take(),
        ),
        Query::select()
            .from(Char::Table)
            .expr_window_name_as(
                Func::count(Expr::col(Char::Id)),
                Alias::new("w"),
                Alias::new("n"),
            )
            .to_string(),
    ]);
}

// [spec:pgorm:req:sql.render.oracle/test]    INSERT, including ON CONFLICT and RETURNING
// [spec:pgorm:req:sql.render.insert/test]
// [spec:pgorm:req:sql.render.on-conflict+1/test]
// [spec:pgorm:req:sql.render.returning/test]
#[test]
fn sweep_insert_shapes() {
    let insert = || {
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect, Glyph::Image])
            .values_panic([1.into(), "a".into()])
            .to_owned()
    };

    sweep([
        insert().to_string(),
        insert().values_panic([2.into(), "b".into()]).to_string(),
        Query::insert()
            .into_table(Glyph::Table)
            .or_default_values_many(3)
            .to_string(),
        insert().returning(Query::returning().all()).to_string(),
        insert()
            .returning(Query::returning().columns([Glyph::Id, Glyph::Image]))
            .to_string(),
        insert()
            .returning(Query::returning().exprs([Expr::col(Glyph::Id).add(1)]))
            .to_string(),
        insert().on_conflict(OnConflict::do_nothing()).to_string(),
        insert()
            .on_conflict(OnConflict::column(Glyph::Id).do_nothing())
            .to_string(),
        insert()
            .on_conflict(
                OnConflict::column(Glyph::Id)
                    .and_column(Glyph::Aspect)
                    .update_column(Glyph::Image),
            )
            .to_string(),
        insert()
            .on_conflict(
                OnConflict::column(Glyph::Id).value(Glyph::Aspect, Expr::col(Glyph::Aspect).add(1)),
            )
            .to_string(),
        insert()
            .on_conflict(
                OnConflict::expr(Func::lower(Expr::col(Glyph::Tokens)))
                    .and_where(Expr::col(Glyph::Aspect).gt(0))
                    .update_column(Glyph::Image)
                    .and_where(Expr::col(Glyph::Id).gt(0)),
            )
            .to_string(),
    ]);
}

// [spec:pgorm:req:sql.render.oracle/test]    UPDATE and DELETE
// [spec:pgorm:req:sql.render.update-delete/test]
#[test]
fn sweep_update_and_delete_shapes() {
    sweep([
        Query::update()
            .table(Glyph::Table)
            .value(Glyph::Aspect, Expr::col(Glyph::Aspect).add(1))
            .values([(Glyph::Image, "a".into())])
            .and_where(Expr::col(Glyph::Id).eq(1))
            .to_string(),
        Query::update()
            .table(Glyph::Table)
            .value(Glyph::Aspect, 1)
            .returning(Query::returning().all())
            .to_string(),
        Query::delete()
            .from_table(Glyph::Table)
            .and_where(Expr::col(Glyph::Id).is_in([1, 2]))
            .to_string(),
        Query::delete()
            .from_table(Glyph::Table)
            .returning(Query::returning().columns([Glyph::Id]))
            .to_string(),
    ]);
}

// [spec:pgorm:req:sql.render.oracle/test]    table DDL
// [spec:pgorm:req:sql.ddl.create-table+6/test]
#[test]
fn sweep_table_ddl_shapes() {
    sweep([
        Table::create(Glyph::Table)
            .if_not_exists()
            .col(
                ColumnDef::new(Glyph::Id)
                    .integer()
                    .not_null()
                    .auto_increment()
                    .primary_key(),
            )
            .col(ColumnDef::new(Glyph::Aspect).double().default(1.0))
            .col(ColumnDef::new(Glyph::Image).text().unique_key())
            .col(
                ColumnDef::new(Glyph::Tokens)
                    .json_binary()
                    .check(Expr::col(Glyph::Aspect).gt(0)),
            )
            .index(
                &mut Index::create(Glyph::Table, Glyph::Id)
                    .name("glyph_pk")
                    .primary()
                    .take(),
            )
            .foreign_key(
                &mut ForeignKey::create()
                    .from(Glyph::Table, Glyph::Id)
                    .to(Font::Table, Font::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .take(),
            )
            .to_string(),
        Table::create(Glyph::Table)
            .col(
                ColumnDef::new(Glyph::Aspect)
                    .integer()
                    .generated(Expr::val(1), true),
            )
            .to_string(),
        Table::alter(Glyph::Table)
            .add_column(ColumnDef::new(Alias::new("added")).integer().not_null())
            .to_string(),
        Table::alter(Glyph::Table)
            .modify_column(ColumnDef::new(Glyph::Aspect).big_integer())
            .to_string(),
        Table::alter(Glyph::Table)
            .modify_column(ColumnDef::new(Glyph::Aspect).null())
            .to_string(),
        Table::rename_column(Glyph::Table, Glyph::Aspect, Alias::new("ratio")).to_string(),
        Table::alter(Glyph::Table)
            .drop_column(Glyph::Aspect)
            .to_string(),
        Table::alter(Char::Table)
            .drop_foreign_key(Alias::new("fk"))
            .to_string(),
        Table::rename(Glyph::Table, Alias::new("glyph_old")).to_string(),
        Table::truncate(Glyph::Table).to_string(),
        Table::drop(Glyph::Table).if_exists().cascade().to_string(),
        Table::drop(Glyph::Table)
            .table(Font::Table)
            .restrict()
            .to_string(),
    ]);
}

// [spec:pgorm:req:sql.render.oracle/test]    index, foreign-key, type, extension and comment DDL
// [spec:pgorm:req:sql.ddl.index-create+4/test]
#[test]
fn sweep_schema_object_ddl_shapes() {
    sweep([
        Index::create(Glyph::Table, Glyph::Aspect)
            .name("idx")
            .to_string(),
        Index::create((Alias::new("public"), Glyph::Table), Glyph::Aspect)
            .if_not_exists()
            .unique()
            .nulls_not_distinct()
            .name("idx")
            .col(Glyph::Image)
            .to_string(),
        Index::create(Glyph::Table, Glyph::Tokens)
            .name("idx")
            .index_type(IndexType::FullText)
            .to_string(),
        Index::create(Glyph::Table, Glyph::Aspect)
            .name("idx")
            .index_type(IndexType::Hash)
            .to_string(),
        Index::drop("idx").to_string(),
        ForeignKey::create()
            .name("fk")
            .from(Char::Table, Char::FontId)
            .to(Font::Table, Font::Id)
            .on_delete(ForeignKeyAction::SetNull)
            .on_update(ForeignKeyAction::NoAction)
            .to_string(),
        ForeignKey::drop(Char::Table, "fk").to_string(),
        Type::create()
            .as_enum(Alias::new("tea"))
            .values([Alias::new("breakfast"), Alias::new("earl grey")])
            .to_string(),
        Type::alter()
            .name(Alias::new("tea"))
            .add_value(Alias::new("oolong"))
            .to_string(),
        Type::alter()
            .name(Alias::new("tea"))
            .add_value(Alias::new("oolong"))
            .after(Alias::new("breakfast"))
            .to_string(),
        Type::alter()
            .name(Alias::new("tea"))
            .rename_value(Alias::new("oolong"), Alias::new("wulong"))
            .to_string(),
        Type::drop()
            .if_exists()
            .name(Alias::new("tea"))
            .cascade()
            .to_string(),
        Extension::create().name("ltree").to_string(),
        Extension::create()
            .name("ltree")
            .schema("public")
            .if_not_exists()
            .cascade()
            .to_string(),
        Extension::drop()
            .name("ltree")
            .if_exists()
            .cascade()
            .to_string(),
        Comment::on_table(Glyph::Table, "one row per glyph").to_string(),
        Comment::on_column(
            (Alias::new("public"), Glyph::Table),
            Glyph::Aspect,
            "it's fine",
        )
        .to_string(),
    ]);
}

// [spec:pgorm:req:sql.render.oracle/test]    every `ColumnType` that has a PostgreSQL spelling
// [spec:pgorm:def:sql.render.ddl.types+3/test]
#[test]
fn sweep_column_type_vocabulary() {
    let types = [
        ColumnType::Char(None),
        ColumnType::Char(Some(4)),
        ColumnType::String(StringLen::None),
        ColumnType::String(StringLen::N(255)),
        ColumnType::Text,
        ColumnType::SmallInteger,
        ColumnType::Integer,
        ColumnType::BigInteger,
        ColumnType::Float,
        ColumnType::Double,
        ColumnType::Decimal(None),
        ColumnType::Decimal(Some((12, 2))),
        ColumnType::Timestamp,
        ColumnType::TimestampWithTimeZone,
        ColumnType::Time,
        ColumnType::Date,
        ColumnType::Interval(IntervalSpec::Any(None)),
        ColumnType::Interval(IntervalSpec::Any(Some(IntervalPrecision::P6))),
        ColumnType::Interval(IntervalSpec::Fields(PgInterval::YearToMonth)),
        ColumnType::Interval(IntervalSpec::Fields(PgInterval::MinuteToSecond(Some(
            IntervalPrecision::P2,
        )))),
        ColumnType::Bytea,
        ColumnType::Bit(None),
        ColumnType::Bit(Some(8)),
        ColumnType::VarBit(8),
        ColumnType::Boolean,
        ColumnType::Money,
        ColumnType::Json,
        ColumnType::JsonBinary,
        ColumnType::Uuid,
        ColumnType::Array(RcOrArc::new(ColumnType::Integer)),
        ColumnType::Vector(Some(3)),
        ColumnType::Cidr,
        ColumnType::Inet,
        ColumnType::MacAddr,
        ColumnType::LTree,
    ];

    sweep(types.into_iter().map(|column_type| {
        Table::create(Glyph::Table)
            .col(ColumnDef::new_with_type(Alias::new("c"), column_type))
            .to_string()
    }));
}

// [spec:pgorm:req:sql.render.oracle/test]    the binary operator vocabulary, minus `Escape`, which
// is only grammatical inside LIKE and is pinned in `oracle_pins.rs`
// [spec:pgorm:def:sql.render.operators+2/test]
#[test]
fn sweep_binary_operator_vocabulary() {
    let opers = [
        BinOper::And,
        BinOper::Or,
        BinOper::Like,
        BinOper::NotLike,
        BinOper::ILike,
        BinOper::NotILike,
        BinOper::Is,
        BinOper::IsNot,
        BinOper::In,
        BinOper::NotIn,
        BinOper::Equal,
        BinOper::NotEqual,
        BinOper::SmallerThan,
        BinOper::GreaterThan,
        BinOper::SmallerThanOrEqual,
        BinOper::GreaterThanOrEqual,
        BinOper::Add,
        BinOper::Sub,
        BinOper::Mul,
        BinOper::Div,
        BinOper::Mod,
        BinOper::LShift,
        BinOper::RShift,
        BinOper::Matches,
        BinOper::Contains,
        BinOper::Contained,
        BinOper::Concatenate,
        BinOper::Overlap,
        BinOper::Similarity,
        BinOper::WordSimilarity,
        BinOper::StrictWordSimilarity,
        BinOper::SimilarityDistance,
        BinOper::WordSimilarityDistance,
        BinOper::StrictWordSimilarityDistance,
        BinOper::GetJsonField,
        BinOper::CastJsonField,
        BinOper::Regex,
        BinOper::RegexCaseInsensitive,
        BinOper::EuclideanDistance,
        BinOper::NegativeInnerProduct,
        BinOper::CosineDistance,
        BinOper::Custom("~~"),
    ];

    sweep(opers.into_iter().map(|oper| {
        let right: SimpleExpr = match oper {
            BinOper::Is | BinOper::IsNot => SimpleExpr::Keyword(Keyword::Null),
            BinOper::In | BinOper::NotIn => {
                Expr::tuple([Expr::val(1).into(), Expr::val(2).into()]).into()
            }
            _ => Expr::val(1).into(),
        };
        Query::select()
            .expr(Expr::col(Glyph::Aspect).binary(oper, right))
            .from(Glyph::Table)
            .to_string()
    }));
}

// [spec:pgorm:req:sql.render.oracle/test]    the `build()` path: `$N` placeholders parse as
// PostgreSQL parameter references
// [spec:pgorm:req:sql.render.placeholders/test]
#[test]
fn sweep_placeholder_builds() {
    let (select, _) = base()
        .and_where(Expr::col(Glyph::Aspect).eq(1))
        .and_where(Expr::col(Glyph::Image).like("a%"))
        .limit(10)
        .offset(2)
        .build();
    let (insert, _) = Query::insert()
        .into_table(Glyph::Table)
        .columns([Glyph::Aspect, Glyph::Image])
        .values_panic([1.into(), "a".into()])
        .on_conflict(OnConflict::column(Glyph::Id).update_column(Glyph::Image))
        .returning(Query::returning().all())
        .build();
    let (update, _) = Query::update()
        .table(Glyph::Table)
        .value(Glyph::Aspect, 1)
        .and_where(Expr::col(Glyph::Id).eq(2))
        .build();
    let (delete, _) = Query::delete()
        .from_table(Glyph::Table)
        .and_where(Expr::col(Glyph::Id).eq(2))
        .build();
    let (cast, _) = Query::select()
        .expr(Func::cast_as(Expr::val(1), Alias::new("text")))
        .build();

    sweep([select, insert, update, delete, cast]);
}

// [spec:pgorm:req:sql.render.oracle/test]    `assert_query_eq` is the paired helper: it holds a
// rendered statement to both its expected spelling and the grammar
#[test]
fn oracle_pairs_text_and_grammar_checks() {
    assert_query_eq(&base().to_string(), r#"SELECT "id" FROM "glyph""#);
    assert_query_eq(
        &Table::truncate(Glyph::Table).to_string(),
        r#"TRUNCATE TABLE "glyph""#,
    );
}

// [spec:pgorm:req:sql.render.oracle/test]    both arms of the shim's type dispatch reach the
// oracle, so a silent regression in method resolution cannot mute the retrofitted assertions
#[test]
#[should_panic(expected = "PostgreSQL rejected")]
fn oracle_shim_fires_on_a_string() {
    let rendered = Query::update()
        .table(Glyph::Table)
        .value(Glyph::Aspect, 1)
        .order_by(Glyph::Id, Order::Asc)
        .limit(1)
        .to_string();
    crate::oracle::assert_eq!(rendered, rendered.clone());
}

// [spec:pgorm:req:sql.render.oracle/test]
#[test]
#[should_panic(expected = "PostgreSQL rejected")]
fn oracle_shim_fires_on_a_str_slice() {
    crate::oracle::assert_eq!("SELECT FROM WHERE", "SELECT FROM WHERE");
}
