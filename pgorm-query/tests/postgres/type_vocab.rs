use super::*;
use crate::oracle::{assert_eq, assert_eq_unparsed};

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
            .to_string(),
        r#"SELECT "id", "glyph"."id", "schema"."glyph"."id", *, "glyph".*"#
    );
}

// [spec:pgorm:def:sql.types.table-ref+2/test]    `IntoTableName` maps iden / 2-tuple
#[test]
fn into_table_name_maps_the_two_forms() {
    assert_eq!(
        Glyph::Table.into_table_name(),
        TableName::Table(Glyph::Table.into_iden())
    );
    assert_eq!(
        (Alias::new("schema"), Glyph::Table).into_table_name(),
        TableName::SchemaTable(Alias::new("schema").into_iden(), Glyph::Table.into_iden())
    );
}

// [spec:pgorm:def:sql.types.table-ref+2/test]    `IntoNamedTable` maps the same spellings to an
// unaliased named table, and a `TableName` or `NamedTable` passes through
#[test]
fn into_named_table_maps_the_named_forms() {
    let unaliased = |name| NamedTable { name, alias: None };

    assert_eq!(
        Glyph::Table.into_named_table(),
        unaliased(TableName::Table(Glyph::Table.into_iden()))
    );
    assert_eq!(
        (Alias::new("schema"), Glyph::Table).into_named_table(),
        unaliased(TableName::SchemaTable(
            Alias::new("schema").into_iden(),
            Glyph::Table.into_iden()
        ))
    );
    assert_eq!(
        Glyph::Table.into_table_name().into_named_table(),
        Glyph::Table.into_named_table()
    );
    assert_eq!(
        Glyph::Table
            .into_named_table()
            .alias(Alias::new("g"))
            .into_named_table(),
        Glyph::Table.into_named_table().alias(Alias::new("g"))
    );
    assert_eq!(
        NamedTable::from(Glyph::Table.into_table_name()),
        Glyph::Table.into_named_table()
    );
}

// [spec:pgorm:def:sql.types.table-ref+2/test]    `IntoFromItem` widens every named spelling, and
// a `TableName` or `NamedTable` converts infallibly
#[test]
fn into_from_item_maps_the_named_forms() {
    assert_eq!(
        Glyph::Table.into_from_item(),
        FromItem::Table(NamedTable {
            name: TableName::Table(Glyph::Table.into_iden()),
            alias: None,
        })
    );
    assert_eq!(
        (Alias::new("schema"), Glyph::Table).into_from_item(),
        FromItem::Table(NamedTable {
            name: TableName::SchemaTable(
                Alias::new("schema").into_iden(),
                Glyph::Table.into_iden()
            ),
            alias: None,
        })
    );
    assert_eq!(
        FromItem::from((Alias::new("schema"), Glyph::Table).into_table_name()),
        (Alias::new("schema"), Glyph::Table).into_from_item()
    );
    assert_eq!(
        FromItem::from(Glyph::Table.into_named_table().alias(Alias::new("g"))),
        Glyph::Table.into_from_item().alias(Alias::new("g"))
    );
}

// [spec:pgorm:def:sql.types.table-ref+2/test]    `alias` binds an alias and replaces an existing
// one, on the named form and on the value-producing forms alike
#[test]
fn from_item_alias_adds_or_replaces() {
    let named = |alias: &str| {
        FromItem::Table(NamedTable {
            name: TableName::Table(Glyph::Table.into_iden()),
            alias: Some(Alias::new(alias).into_iden()),
        })
    };

    assert_eq!(
        Glyph::Table.into_from_item().alias(Alias::new("g")),
        named("g")
    );
    assert_eq!(
        Glyph::Table
            .into_from_item()
            .alias(Alias::new("g"))
            .alias(Alias::new("h")),
        named("h")
    );
    assert_eq!(
        (Alias::new("schema"), Glyph::Table)
            .into_from_item()
            .alias(Alias::new("g")),
        FromItem::Table(NamedTable {
            name: TableName::SchemaTable(
                Alias::new("schema").into_iden(),
                Glyph::Table.into_iden()
            ),
            alias: Some(Alias::new("g").into_iden()),
        })
    );
    assert_eq!(
        FromItem::ValuesList(vec![], Alias::new("v").into_iden()).alias(Alias::new("w")),
        FromItem::ValuesList(vec![], Alias::new("w").into_iden())
    );
}

// [spec:pgorm:def:sql.types.table-ref+2/test]    a column of a from item is qualified by its
// alias when it has one, otherwise by the table it names
#[test]
fn from_item_qualifier_prefers_the_alias() {
    let named = (Alias::new("schema"), Glyph::Table).into_from_item();
    assert_eq!(named.qualifier().to_string(), "glyph");
    assert_eq!(
        named.clone().alias(Alias::new("g")).qualifier().to_string(),
        "g"
    );
    assert_eq!(
        named.table_name(),
        Some(&(Alias::new("schema"), Glyph::Table).into_table_name())
    );

    let table = (Alias::new("schema"), Glyph::Table).into_named_table();
    assert_eq!(table.qualifier().to_string(), "glyph");
    assert_eq!(table.alias(Alias::new("g")).qualifier().to_string(), "g");

    let values = FromItem::ValuesList(vec![], Alias::new("v").into_iden());
    assert_eq!(values.qualifier().to_string(), "v");
    assert_eq!(values.table_name(), None);
}

// [spec:pgorm:def:sql.types.table-ref+2/test]    the named form renders as dotted, quoted parts
// with an optional alias
#[test]
fn named_from_item_forms_render() {
    let rendered =
        |from_item: FromItem| Query::select().column(Asterisk).from(from_item).to_string();

    assert_eq!(
        rendered(Glyph::Table.into_from_item()),
        r#"SELECT * FROM "glyph""#
    );
    assert_eq!(
        rendered((Alias::new("schema"), Glyph::Table).into_from_item()),
        r#"SELECT * FROM "schema"."glyph""#
    );
    assert_eq!(
        rendered(Glyph::Table.into_from_item().alias(Alias::new("g"))),
        r#"SELECT * FROM "glyph" AS "g""#
    );
    assert_eq!(
        rendered(
            (Alias::new("schema"), Glyph::Table)
                .into_from_item()
                .alias(Alias::new("g"))
        ),
        r#"SELECT * FROM "schema"."glyph" AS "g""#
    );
}

// [spec:pgorm:def:sql.types.table-ref+2/test]    the write statements take the same named table,
// and PostgreSQL accepts the alias each of them renders
// [spec:pgorm:def:sql.ast.insert+1/test]
// [spec:pgorm:req:sql.ast.update+1/test]
// [spec:pgorm:def:sql.ast.delete+1/test]
#[test]
fn aliased_dml_targets_render() {
    let target = || {
        (Alias::new("schema"), Glyph::Table)
            .into_named_table()
            .alias(Alias::new("g"))
    };

    assert_eq!(
        Query::insert()
            .into_table(target())
            .columns([Glyph::Image])
            .values_panic(["12A".into()])
            .to_string(),
        r#"INSERT INTO "schema"."glyph" AS "g" ("image") VALUES ('12A')"#
    );
    assert_eq!(
        Query::update()
            .table(target())
            .value(Glyph::Aspect, 1.23)
            .and_where(Expr::col((Alias::new("g"), Glyph::Id)).eq(1))
            .to_string(),
        r#"UPDATE "schema"."glyph" AS "g" SET "aspect" = 1.23 WHERE "g"."id" = 1"#
    );
    assert_eq!(
        Query::delete()
            .from_table(target())
            .and_where(Expr::col((Alias::new("g"), Glyph::Id)).eq(1))
            .to_string(),
        r#"DELETE FROM "schema"."glyph" AS "g" WHERE "g"."id" = 1"#
    );
}

// [spec:pgorm:def:sql.types.table-ref+2/test]    the three value-producing forms, all with a
// mandatory alias
#[test]
fn value_producing_from_item_forms_render() {
    let sub_query = FromItem::SubQuery(
        Query::select().column(Glyph::Id).from(Glyph::Table).take(),
        Alias::new("sub").into_iden(),
    );
    assert_eq!(
        Query::select().column(Asterisk).from(sub_query).to_string(),
        r#"SELECT * FROM (SELECT "id" FROM "glyph") AS "sub""#
    );

    let values_list = FromItem::ValuesList(
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
            .to_string(),
        r#"SELECT * FROM (VALUES (1, 'a'), (2, 'b')) AS "v""#
    );

    let function_call = FromItem::FunctionCall(
        Func::cust(Alias::new("generate_series")).arg(1i32),
        Alias::new("f").into_iden(),
    );
    assert_eq!(
        Query::select()
            .column(Asterisk)
            .from(function_call)
            .to_string(),
        r#"SELECT * FROM generate_series(1) AS "f""#
    );
}

// [spec:pgorm:def:sql.types.opers+1/test]    `Not` is the only unary operator
#[test]
fn the_only_unary_operator_is_not() {
    let not_true = SimpleExpr::Unary(UnOper::Not, Box::new(SimpleExpr::from(true)));

    assert_eq!(
        Query::select().expr(not_true).to_string(),
        "SELECT NOT TRUE"
    );
}

// [spec:pgorm:def:sql.types.opers+1/test]    the whole binary operator vocabulary, including the
// `Custom` escape hatch
#[test]
fn the_binary_operator_vocabulary_is_complete() {
    let rendered = |op: BinOper| {
        Query::select()
            .expr(Expr::col(Glyph::Aspect).binary(op, Expr::val(1)))
            .to_string()
    };

    for (op, lexeme) in [
        (BinOper::And, "AND"),
        (BinOper::Or, "OR"),
        (BinOper::Like, "LIKE"),
        (BinOper::NotLike, "NOT LIKE"),
        (BinOper::ILike, "ILIKE"),
        (BinOper::NotILike, "NOT ILIKE"),
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
        assert_eq_unparsed!(
            rendered(op),
            format!(r#"SELECT "aspect" {lexeme} 1"#),
            "unexpected rendering for {op:?}"
        );
    }

    // `As` is the cast encoding: its right operand is a raw Custom expression.
    assert_eq!(
        Query::select()
            .expr(Expr::col(Glyph::Aspect).binary(BinOper::As, Expr::cust("text")))
            .to_string(),
        r#"SELECT "aspect" AS text"#
    );
}

// [spec:pgorm:def:sql.types.column-type+3/test]    `StringLen` parameterises varchar and the
// convenience constructors go through it
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
        ColumnType::custom("citext"),
        ColumnType::Custom(Alias::new("citext").into_iden())
    );
}

// [spec:pgorm:req:sql.ddl.column-def+3/test]    only the integer trio has a serial spelling
#[test]
fn serial_spelling_covers_the_integer_trio() {
    assert_eq!(
        ColumnType::SmallInteger.serial_spelling(),
        Some("smallserial")
    );
    assert_eq!(ColumnType::Integer.serial_spelling(), Some("serial"));
    assert_eq!(ColumnType::BigInteger.serial_spelling(), Some("bigserial"));

    for other in [ColumnType::Uuid, ColumnType::Text, ColumnType::Bytea] {
        assert_eq!(other.serial_spelling(), None);
    }
}

// [spec:pgorm:req:sql.ddl.column-def+3/test]    a type with no serial form renders itself
#[test]
fn auto_increment_without_serial_form_renders_type() {
    assert_eq!(
        Table::create(Glyph::Table)
            .col(ColumnDef::new(Glyph::Id).uuid().auto_increment())
            .to_string(),
        r#"CREATE TABLE "glyph" ( "id" uuid )"#
    );
}

// [spec:pgorm:def:sql.types.column-type+3/test]    equality compares parameters, renders
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
        ColumnType::Interval(IntervalSpec::Fields(PgInterval::Second(Some(
            IntervalPrecision::P3
        )))),
        ColumnType::Interval(IntervalSpec::Fields(PgInterval::Second(Some(
            IntervalPrecision::P3
        ))))
    );
    assert_ne!(
        ColumnType::Interval(IntervalSpec::Fields(PgInterval::Hour)),
        ColumnType::Interval(IntervalSpec::Fields(PgInterval::Day))
    );
    assert_ne!(
        ColumnType::Interval(IntervalSpec::Any(None)),
        ColumnType::Interval(IntervalSpec::Any(Some(IntervalPrecision::P0)))
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

// [spec:pgorm:def:sql.types.column-type+3/test]    `PgInterval` displays as SQL keywords and
// has a case-insensitive `TryFrom<&str>` inverse
#[test]
fn pg_interval_display_and_parse_round_trip() {
    let all = [
        (PgInterval::Year, "YEAR"),
        (PgInterval::Month, "MONTH"),
        (PgInterval::Day, "DAY"),
        (PgInterval::Hour, "HOUR"),
        (PgInterval::Minute, "MINUTE"),
        (PgInterval::Second(None), "SECOND"),
        (PgInterval::YearToMonth, "YEAR TO MONTH"),
        (PgInterval::DayToHour, "DAY TO HOUR"),
        (PgInterval::DayToMinute, "DAY TO MINUTE"),
        (PgInterval::DayToSecond(None), "DAY TO SECOND"),
        (PgInterval::HourToMinute, "HOUR TO MINUTE"),
        (PgInterval::HourToSecond(None), "HOUR TO SECOND"),
        (PgInterval::MinuteToSecond(None), "MINUTE TO SECOND"),
    ];

    for (field, keywords) in all {
        assert_eq!(field.to_string(), keywords);
        assert_eq!(PgInterval::try_from(keywords).unwrap(), field);
    }

    // A precision is spelled by the field that carries it.
    assert_eq!(
        PgInterval::MinuteToSecond(Some(IntervalPrecision::P6)).to_string(),
        "MINUTE TO SECOND(6)"
    );

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

// [spec:pgorm:def:sql.types.column-type+3/test]    the precision vocabulary is the closed set
// PostgreSQL accepts, and nothing outside it constructs
#[test]
fn interval_precision_is_zero_through_six() {
    for digits in 0..=6u8 {
        let precision = IntervalPrecision::new(digits).expect("0..=6 are precisions");
        assert_eq!(precision.digits(), digits);
        assert_eq!(precision.to_string(), digits.to_string());
    }

    assert_eq!(IntervalPrecision::new(7), None);
    assert_eq!(IntervalPrecision::new(u8::MAX), None);
}
