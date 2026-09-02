use super::*;
use pretty_assertions::assert_eq;

// [spec:pgorm:def:sql.types.column-ref/test]    the five forms and what `IntoColumnRef` maps onto them
#[test]
fn into_column_ref_maps_every_form() {
    assert_eq!(
        Glyph::Id.into_column_ref(),
        ColumnRef::Column(Glyph::Id.into_iden())
    );
    assert_eq!(
        (Glyph::Table, Glyph::Id).into_column_ref(),
        ColumnRef::TableColumn(Glyph::Table.into_iden(), Glyph::Id.into_iden())
    );
    assert_eq!(
        (Alias::new("schema"), Glyph::Table, Glyph::Id).into_column_ref(),
        ColumnRef::SchemaTableColumn(
            Alias::new("schema").into_iden(),
            Glyph::Table.into_iden(),
            Glyph::Id.into_iden()
        )
    );
    assert_eq!(Asterisk.into_column_ref(), ColumnRef::Asterisk);
    assert_eq!(
        (Glyph::Table, Asterisk).into_column_ref(),
        ColumnRef::TableAsterisk(Glyph::Table.into_iden())
    );

    // An existing ColumnRef passes through unchanged.
    assert_eq!(ColumnRef::Asterisk.into_column_ref(), ColumnRef::Asterisk);
}

// [spec:pgorm:def:sql.types.column-ref/test]    each form is renderable
#[test]
fn every_column_ref_form_renders() {
    assert_eq!(
        Query::select()
            .column(ColumnRef::Column(Glyph::Id.into_iden()))
            .column(ColumnRef::TableColumn(
                Glyph::Table.into_iden(),
                Glyph::Id.into_iden()
            ))
            .column(ColumnRef::SchemaTableColumn(
                Alias::new("schema").into_iden(),
                Glyph::Table.into_iden(),
                Glyph::Id.into_iden()
            ))
            .column(ColumnRef::Asterisk)
            .column(ColumnRef::TableAsterisk(Glyph::Table.into_iden()))
            .to_string(QueryBuilder),
        r#"SELECT "id", "glyph"."id", "schema"."glyph"."id", *, "glyph".*"#
    );
}

// [spec:pgorm:def:sql.types.table-ref/test]    `IntoTableRef` maps iden / 2-tuple / 3-tuple
#[test]
fn into_table_ref_maps_the_plain_forms() {
    assert_eq!(
        Glyph::Table.into_table_ref(),
        TableRef::Table(Glyph::Table.into_iden())
    );
    assert_eq!(
        (Alias::new("schema"), Glyph::Table).into_table_ref(),
        TableRef::SchemaTable(Alias::new("schema").into_iden(), Glyph::Table.into_iden())
    );
    assert_eq!(
        (Alias::new("db"), Alias::new("schema"), Glyph::Table).into_table_ref(),
        TableRef::DatabaseSchemaTable(
            Alias::new("db").into_iden(),
            Alias::new("schema").into_iden(),
            Glyph::Table.into_iden()
        )
    );
}

// [spec:pgorm:def:sql.types.table-ref/test]    `alias` upgrades a plain form and replaces an
// existing alias
#[test]
fn table_ref_alias_adds_or_replaces() {
    assert_eq!(
        Glyph::Table.into_table_ref().alias(Alias::new("g")),
        TableRef::TableAlias(Glyph::Table.into_iden(), Alias::new("g").into_iden())
    );
    assert_eq!(
        Glyph::Table
            .into_table_ref()
            .alias(Alias::new("g"))
            .alias(Alias::new("h")),
        TableRef::TableAlias(Glyph::Table.into_iden(), Alias::new("h").into_iden())
    );
    assert_eq!(
        (Alias::new("schema"), Glyph::Table)
            .into_table_ref()
            .alias(Alias::new("g")),
        TableRef::SchemaTableAlias(
            Alias::new("schema").into_iden(),
            Glyph::Table.into_iden(),
            Alias::new("g").into_iden()
        )
    );
    assert_eq!(
        (Alias::new("db"), Alias::new("schema"), Glyph::Table)
            .into_table_ref()
            .alias(Alias::new("g")),
        TableRef::DatabaseSchemaTableAlias(
            Alias::new("db").into_iden(),
            Alias::new("schema").into_iden(),
            Glyph::Table.into_iden(),
            Alias::new("g").into_iden()
        )
    );
}

// [spec:pgorm:def:sql.types.table-ref/test]    the six identifier forms render as dotted,
// quoted parts with an optional alias
#[test]
fn identifier_table_ref_forms_render() {
    let rendered = |table_ref: TableRef| {
        Query::select()
            .column(Asterisk)
            .from(table_ref)
            .to_string(QueryBuilder)
    };

    assert_eq!(
        rendered(Glyph::Table.into_table_ref()),
        r#"SELECT * FROM "glyph""#
    );
    assert_eq!(
        rendered((Alias::new("schema"), Glyph::Table).into_table_ref()),
        r#"SELECT * FROM "schema"."glyph""#
    );
    assert_eq!(
        rendered((Alias::new("db"), Alias::new("schema"), Glyph::Table).into_table_ref()),
        r#"SELECT * FROM "db"."schema"."glyph""#
    );
    assert_eq!(
        rendered(Glyph::Table.into_table_ref().alias(Alias::new("g"))),
        r#"SELECT * FROM "glyph" AS "g""#
    );
    assert_eq!(
        rendered(
            (Alias::new("schema"), Glyph::Table)
                .into_table_ref()
                .alias(Alias::new("g"))
        ),
        r#"SELECT * FROM "schema"."glyph" AS "g""#
    );
    assert_eq!(
        rendered(
            (Alias::new("db"), Alias::new("schema"), Glyph::Table)
                .into_table_ref()
                .alias(Alias::new("g"))
        ),
        r#"SELECT * FROM "db"."schema"."glyph" AS "g""#
    );
}

// [spec:pgorm:def:sql.types.table-ref/test]    the three value-producing forms, all with a
// mandatory alias
#[test]
fn value_producing_table_ref_forms_render() {
    let sub_query = TableRef::SubQuery(
        Query::select().column(Glyph::Id).from(Glyph::Table).take(),
        Alias::new("sub").into_iden(),
    );
    assert_eq!(
        Query::select()
            .column(Asterisk)
            .from(sub_query)
            .to_string(QueryBuilder),
        r#"SELECT * FROM (SELECT "id" FROM "glyph") AS "sub""#
    );

    let values_list = TableRef::ValuesList(
        vec![
            (1i32, "a").into_value_tuple(),
            (2i32, "b").into_value_tuple(),
        ],
        Alias::new("v").into_iden(),
    );
    assert_eq!(
        Query::select()
            .column(Asterisk)
            .from(values_list)
            .to_string(QueryBuilder),
        r#"SELECT * FROM (VALUES (1, 'a'), (2, 'b')) AS "v""#
    );

    let function_call = TableRef::FunctionCall(
        Func::cust(Alias::new("generate_series")).arg(1i32),
        Alias::new("f").into_iden(),
    );
    assert_eq!(
        Query::select()
            .column(Asterisk)
            .from(function_call)
            .to_string(QueryBuilder),
        r#"SELECT * FROM generate_series(1) AS "f""#
    );
}

// [spec:pgorm:def:sql.types.opers/test]    `Not` is the only unary operator
#[test]
fn the_only_unary_operator_is_not() {
    let not_true = SimpleExpr::Unary(UnOper::Not, Box::new(SimpleExpr::from(true)));

    assert_eq!(
        Query::select().expr(not_true).to_string(QueryBuilder),
        "SELECT NOT TRUE"
    );
}

// [spec:pgorm:def:sql.types.opers/test]    the whole binary operator vocabulary, including the
// `Custom` escape hatch
#[test]
fn the_binary_operator_vocabulary_is_complete() {
    let rendered = |op: BinOper| {
        Query::select()
            .expr(Expr::col(Glyph::Aspect).binary(op, Expr::val(1)))
            .to_string(QueryBuilder)
    };

    for (op, lexeme) in [
        (BinOper::And, "AND"),
        (BinOper::Or, "OR"),
        (BinOper::Like, "LIKE"),
        (BinOper::NotLike, "NOT LIKE"),
        (BinOper::ILike, "ILIKE"),
        (BinOper::NotILike, "NOT ILIKE"),
        (BinOper::Escape, "ESCAPE"),
        (BinOper::Is, "IS"),
        (BinOper::IsNot, "IS NOT"),
        (BinOper::In, "IN"),
        (BinOper::NotIn, "NOT IN"),
        (BinOper::Between, "BETWEEN"),
        (BinOper::NotBetween, "NOT BETWEEN"),
        (BinOper::Equal, "="),
        (BinOper::NotEqual, "<>"),
        (BinOper::SmallerThan, "<"),
        (BinOper::GreaterThan, ">"),
        (BinOper::SmallerThanOrEqual, "<="),
        (BinOper::GreaterThanOrEqual, ">="),
        (BinOper::Add, "+"),
        (BinOper::Sub, "-"),
        (BinOper::Mul, "*"),
        (BinOper::Div, "/"),
        (BinOper::Mod, "%"),
        (BinOper::LShift, "<<"),
        (BinOper::RShift, ">>"),
        (BinOper::Matches, "@@"),
        (BinOper::Contains, "@>"),
        (BinOper::Contained, "<@"),
        (BinOper::Concatenate, "||"),
        (BinOper::Overlap, "&&"),
        (BinOper::Similarity, "%"),
        (BinOper::WordSimilarity, "<%"),
        (BinOper::StrictWordSimilarity, "<<%"),
        (BinOper::SimilarityDistance, "<->"),
        (BinOper::WordSimilarityDistance, "<<->"),
        (BinOper::StrictWordSimilarityDistance, "<<<->"),
        (BinOper::GetJsonField, "->"),
        (BinOper::CastJsonField, "->>"),
        (BinOper::Regex, "~"),
        (BinOper::RegexCaseInsensitive, "~*"),
        (BinOper::EuclideanDistance, "<->"),
        (BinOper::NegativeInnerProduct, "<#>"),
        (BinOper::CosineDistance, "<=>"),
        (BinOper::Custom("~~"), "~~"),
    ] {
        assert_eq!(
            rendered(op),
            format!(r#"SELECT "aspect" {lexeme} 1"#),
            "unexpected rendering for {op:?}"
        );
    }

    // `As` is the cast encoding: its right operand is a raw Custom expression.
    assert_eq!(
        Query::select()
            .expr(Expr::col(Glyph::Aspect).binary(BinOper::As, Expr::cust("text")))
            .to_string(QueryBuilder),
        r#"SELECT "aspect" AS text"#
    );
}

// [spec:pgorm:def:sql.types.column-type+1/test]    `StringLen` parameterises varchar/varbinary
// and the convenience constructors go through it
#[test]
fn string_len_and_the_convenience_constructors() {
    assert_eq!(StringLen::default(), StringLen::None);
    assert_eq!(
        ColumnType::string(Some(64)),
        ColumnType::String(StringLen::N(64))
    );
    assert_eq!(
        ColumnType::string(None),
        ColumnType::String(StringLen::None)
    );
    assert_eq!(
        ColumnType::var_binary(10),
        ColumnType::VarBinary(StringLen::N(10))
    );
    assert_eq!(
        ColumnType::custom("citext"),
        ColumnType::Custom(Alias::new("citext").into_iden())
    );
}

// [spec:pgorm:def:sql.types.column-type+1/test]    equality compares parameters, renders
// `Custom`/`Enum` identifiers, recurses into `Array`, and otherwise compares discriminants
#[test]
fn column_type_equality_semantics() {
    // Parameterised variants compare their parameters.
    assert_eq!(ColumnType::Char(Some(3)), ColumnType::Char(Some(3)));
    assert_ne!(ColumnType::Char(Some(3)), ColumnType::Char(Some(4)));
    assert_eq!(
        ColumnType::Decimal(Some((10, 2))),
        ColumnType::Decimal(Some((10, 2)))
    );
    assert_ne!(
        ColumnType::Decimal(Some((10, 2))),
        ColumnType::Decimal(None)
    );
    assert_eq!(
        ColumnType::Interval(Some(PgInterval::Hour), Some(3)),
        ColumnType::Interval(Some(PgInterval::Hour), Some(3))
    );
    assert_ne!(
        ColumnType::Interval(Some(PgInterval::Hour), None),
        ColumnType::Interval(Some(PgInterval::Day), None)
    );

    // `Custom` compares by rendered identifier, not by concrete iden type.
    assert_eq!(
        ColumnType::custom("aspect"),
        ColumnType::Custom(Glyph::Aspect.into_iden())
    );
    assert_ne!(ColumnType::custom("aspect"), ColumnType::custom("image"));

    // `Enum` compares name and variant list, both by rendered text.
    let tea = ColumnType::Enum {
        name: Alias::new("tea").into_iden(),
        variants: vec![
            Alias::new("green").into_iden(),
            Alias::new("black").into_iden(),
        ],
    };
    let same_tea = ColumnType::Enum {
        name: Alias::new("tea").into_iden(),
        variants: vec![
            Alias::new("green").into_iden(),
            Alias::new("black").into_iden(),
        ],
    };
    let other_tea = ColumnType::Enum {
        name: Alias::new("tea").into_iden(),
        variants: vec![Alias::new("green").into_iden()],
    };
    assert_eq!(tea, same_tea);
    assert_ne!(tea, other_tea);

    // `Array` recurses into its element type.
    assert_eq!(
        ColumnType::Array(RcOrArc::new(ColumnType::Integer)),
        ColumnType::Array(RcOrArc::new(ColumnType::Integer))
    );
    assert_ne!(
        ColumnType::Array(RcOrArc::new(ColumnType::Integer)),
        ColumnType::Array(RcOrArc::new(ColumnType::Text))
    );

    // Everything else compares discriminants.
    assert_eq!(ColumnType::Text, ColumnType::Text);
    assert_ne!(ColumnType::Text, ColumnType::Json);
    assert_ne!(ColumnType::Cidr, ColumnType::Inet);
    assert_ne!(ColumnType::MacAddr, ColumnType::LTree);
}

// [spec:pgorm:def:sql.types.column-type+1/test]    `PgInterval` displays as SQL keywords and
// has a case-insensitive `TryFrom<&str>` inverse
#[test]
fn pg_interval_display_and_parse_round_trip() {
    let all = [
        (PgInterval::Year, "YEAR"),
        (PgInterval::Month, "MONTH"),
        (PgInterval::Day, "DAY"),
        (PgInterval::Hour, "HOUR"),
        (PgInterval::Minute, "MINUTE"),
        (PgInterval::Second, "SECOND"),
        (PgInterval::YearToMonth, "YEAR TO MONTH"),
        (PgInterval::DayToHour, "DAY TO HOUR"),
        (PgInterval::DayToMinute, "DAY TO MINUTE"),
        (PgInterval::DayToSecond, "DAY TO SECOND"),
        (PgInterval::HourToMinute, "HOUR TO MINUTE"),
        (PgInterval::HourToSecond, "HOUR TO SECOND"),
        (PgInterval::MinuteToSecond, "MINUTE TO SECOND"),
    ];

    for (field, keywords) in all {
        assert_eq!(field.to_string(), keywords);
        assert_eq!(PgInterval::try_from(keywords).unwrap(), field);
    }

    // Case and surrounding whitespace are forgiven; anything else is an error.
    assert_eq!(
        PgInterval::try_from("  year to month ").unwrap(),
        PgInterval::YearToMonth
    );
    assert_eq!(
        PgInterval::try_from("century".to_owned()).unwrap_err(),
        "Cannot turn \"CENTURY\" into a Postgres interval field".to_owned()
    );
}
