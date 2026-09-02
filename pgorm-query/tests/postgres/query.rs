use super::*;
use crate::oracle::{assert_eq, assert_eq_unparsed};

// [spec:pgorm:req:sql.ast/test]
// [spec:pgorm:def:sql.ast.select/test]
// [spec:pgorm:req:sql.render.ident-quoting/test]
#[test]
fn select_1() {
    assert_eq!(
        Query::select()
            .columns([Char::Character, Char::SizeW, Char::SizeH])
            .from(Char::Table)
            .limit(10)
            .offset(100)
            .to_string(QueryBuilder),
        r#"SELECT "character", "size_w", "size_h" FROM "character" LIMIT 10 OFFSET 100"#
    );
}

// [spec:pgorm:def:sql.ast.expr/test]
#[test]
fn select_2() {
    assert_eq!(
        Query::select()
            .columns([Char::Character, Char::SizeW, Char::SizeH])
            .from(Char::Table)
            .and_where(Expr::col(Char::SizeW).eq(3))
            .to_string(QueryBuilder),
        r#"SELECT "character", "size_w", "size_h" FROM "character" WHERE "size_w" = 3"#
    );
}

#[test]
fn select_3() {
    assert_eq!(
        Query::select()
            .columns([Char::Character, Char::SizeW, Char::SizeH])
            .from(Char::Table)
            .and_where(Expr::col(Char::SizeW).eq(3))
            .and_where(Expr::col(Char::SizeH).eq(4))
            .to_string(QueryBuilder),
        r#"SELECT "character", "size_w", "size_h" FROM "character" WHERE "size_w" = 3 AND "size_h" = 4"#
    );
}

// [spec:pgorm:req:sql.ast.select.from/test]
#[test]
fn select_4() {
    assert_eq!(
        Query::select()
            .columns([Glyph::Aspect])
            .from_subquery(
                Query::select()
                    .columns([Glyph::Image, Glyph::Aspect])
                    .from(Glyph::Table)
                    .take(),
                Alias::new("subglyph")
            )
            .to_string(QueryBuilder),
        r#"SELECT "aspect" FROM (SELECT "image", "aspect" FROM "glyph") AS "subglyph""#
    );
}

// [spec:pgorm:req:sql.ast.expr.in+1/test]
#[test]
fn select_5() {
    assert_eq!(
        Query::select()
            .column((Glyph::Table, Glyph::Image))
            .from(Glyph::Table)
            .and_where(Expr::col((Glyph::Table, Glyph::Aspect)).is_in([3, 4]))
            .to_string(QueryBuilder),
        r#"SELECT "glyph"."image" FROM "glyph" WHERE "glyph"."aspect" IN (3, 4)"#
    );
}

// [spec:pgorm:req:sql.render.condition-chain/test]
#[test]
fn select_6() {
    assert_eq!(
        Query::select()
            .columns([Glyph::Aspect])
            .exprs([Expr::col(Glyph::Image).max()])
            .from(Glyph::Table)
            .group_by_columns([Glyph::Aspect])
            .and_having(Expr::col(Glyph::Aspect).gt(2))
            .to_string(QueryBuilder),
        r#"SELECT "aspect", MAX("image") FROM "glyph" GROUP BY "aspect" HAVING "aspect" > 2"#
    );
}

#[test]
fn select_7() {
    assert_eq!(
        Query::select()
            .columns([Glyph::Aspect])
            .from(Glyph::Table)
            .and_where(Expr::expr(Expr::col(Glyph::Aspect).if_null(0)).gt(2))
            .to_string(QueryBuilder),
        r#"SELECT "aspect" FROM "glyph" WHERE COALESCE("aspect", 0) > 2"#
    );
}

// [spec:pgorm:req:sql.render.joins/test]
#[test]
fn select_8() {
    assert_eq!(
        Query::select()
            .columns([Char::Character])
            .from(Char::Table)
            .left_join(
                Font::Table,
                Expr::col((Char::Table, Char::FontId)).equals((Font::Table, Font::Id))
            )
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" LEFT JOIN "font" ON "character"."font_id" = "font"."id""#
    );
}

// [spec:pgorm:req:sql.ast.select.join/test]
#[test]
fn select_9() {
    assert_eq!(
        Query::select()
            .columns([Char::Character])
            .from(Char::Table)
            .left_join(
                Font::Table,
                Expr::col((Char::Table, Char::FontId)).equals((Font::Table, Font::Id))
            )
            .inner_join(
                Glyph::Table,
                Expr::col((Char::Table, Char::Character)).equals((Glyph::Table, Glyph::Image))
            )
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" LEFT JOIN "font" ON "character"."font_id" = "font"."id" INNER JOIN "glyph" ON "character"."character" = "glyph"."image""#
    );
}

#[test]
fn select_10() {
    assert_eq!(
        Query::select()
            .columns([Char::Character])
            .from(Char::Table)
            .left_join(
                Font::Table,
                Expr::col((Char::Table, Char::FontId))
                    .equals((Font::Table, Font::Id))
                    .and(Expr::col((Char::Table, Char::FontId)).equals((Font::Table, Font::Id)))
            )
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" LEFT JOIN "font" ON "character"."font_id" = "font"."id" AND "character"."font_id" = "font"."id""#
    );
}

#[test]
fn select_11() {
    assert_eq!(
        Query::select()
            .columns([Glyph::Aspect])
            .from(Glyph::Table)
            .and_where(Expr::expr(Expr::col(Glyph::Aspect).if_null(0)).gt(2))
            .order_by(Glyph::Image, Order::Desc)
            .order_by((Glyph::Table, Glyph::Aspect), Order::Asc)
            .to_string(QueryBuilder),
        r#"SELECT "aspect" FROM "glyph" WHERE COALESCE("aspect", 0) > 2 ORDER BY "image" DESC, "glyph"."aspect" ASC"#
    );
}

#[test]
fn select_12() {
    assert_eq!(
        Query::select()
            .columns([Glyph::Aspect])
            .from(Glyph::Table)
            .and_where(Expr::expr(Expr::col(Glyph::Aspect).if_null(0)).gt(2))
            .order_by_columns([(Glyph::Id, Order::Asc), (Glyph::Aspect, Order::Desc)])
            .to_string(QueryBuilder),
        r#"SELECT "aspect" FROM "glyph" WHERE COALESCE("aspect", 0) > 2 ORDER BY "id" ASC, "aspect" DESC"#
    );
}

#[test]
fn select_13() {
    assert_eq!(
        Query::select()
            .columns([Glyph::Aspect])
            .from(Glyph::Table)
            .and_where(Expr::expr(Expr::col(Glyph::Aspect).if_null(0)).gt(2))
            .order_by_columns([
                ((Glyph::Table, Glyph::Id), Order::Asc),
                ((Glyph::Table, Glyph::Aspect), Order::Desc),
            ])
            .to_string(QueryBuilder),
        r#"SELECT "aspect" FROM "glyph" WHERE COALESCE("aspect", 0) > 2 ORDER BY "glyph"."id" ASC, "glyph"."aspect" DESC"#
    );
}

#[test]
fn select_14() {
    assert_eq!(
        Query::select()
            .columns([Glyph::Id, Glyph::Aspect])
            .expr(Expr::col(Glyph::Image).max())
            .from(Glyph::Table)
            .group_by_columns([(Glyph::Table, Glyph::Id), (Glyph::Table, Glyph::Aspect)])
            .and_having(Expr::col(Glyph::Aspect).gt(2))
            .to_string(QueryBuilder),
        r#"SELECT "id", "aspect", MAX("image") FROM "glyph" GROUP BY "glyph"."id", "glyph"."aspect" HAVING "aspect" > 2"#
    );
}

#[test]
fn select_15() {
    assert_eq!(
        Query::select()
            .columns([Char::Character])
            .from(Char::Table)
            .and_where(Expr::col(Char::FontId).is_null())
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" WHERE "font_id" IS NULL"#
    );
}

#[test]
fn select_16() {
    assert_eq!(
        Query::select()
            .columns([Char::Character])
            .from(Char::Table)
            .and_where(Expr::col(Char::FontId).is_null())
            .and_where(Expr::col(Char::Character).is_not_null())
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" WHERE "font_id" IS NULL AND "character" IS NOT NULL"#
    );
}

#[test]
fn select_17() {
    assert_eq!(
        Query::select()
            .columns([(Glyph::Table, Glyph::Image)])
            .from(Glyph::Table)
            .and_where(Expr::col((Glyph::Table, Glyph::Aspect)).between(3, 5))
            .to_string(QueryBuilder),
        r#"SELECT "glyph"."image" FROM "glyph" WHERE "glyph"."aspect" BETWEEN 3 AND 5"#
    );
}

#[test]
fn select_18() {
    assert_eq!(
        Query::select()
            .columns([Glyph::Aspect])
            .from(Glyph::Table)
            .and_where(Expr::col(Glyph::Aspect).between(3, 5))
            .and_where(Expr::col(Glyph::Aspect).not_between(8, 10))
            .to_string(QueryBuilder),
        r#"SELECT "aspect" FROM "glyph" WHERE ("aspect" BETWEEN 3 AND 5) AND ("aspect" NOT BETWEEN 8 AND 10)"#
    );
}

#[test]
fn select_19() {
    assert_eq!(
        Query::select()
            .columns([Char::Character])
            .from(Char::Table)
            .and_where(Expr::col(Char::Character).eq("A"))
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" WHERE "character" = 'A'"#
    );
}

#[test]
fn select_20() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .and_where(Expr::col(Char::Character).like("A"))
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" WHERE "character" LIKE 'A'"#
    );
}

#[test]
fn select_21() {
    assert_eq!(
        Query::select()
            .columns([Char::Character])
            .from(Char::Table)
            .cond_where(any![
                Expr::col(Char::Character).like("A%"),
                Expr::col(Char::Character).like("%B"),
                Expr::col(Char::Character).like("%C%"),
            ])
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" WHERE "character" LIKE 'A%' OR "character" LIKE '%B' OR "character" LIKE '%C%'"#
    );
}

// [spec:pgorm:def:sql.ast.condition/test]
#[test]
fn select_22() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .cond_where(
                Cond::all()
                    .add(
                        Cond::any().add(Expr::col(Char::Character).like("C")).add(
                            Expr::col(Char::Character)
                                .like("D")
                                .and(Expr::col(Char::Character).like("E"))
                        )
                    )
                    .add(
                        Expr::col(Char::Character)
                            .like("F")
                            .or(Expr::col(Char::Character).like("G"))
                    )
            )
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" WHERE ("character" LIKE 'C' OR ("character" LIKE 'D' AND "character" LIKE 'E')) AND ("character" LIKE 'F' OR "character" LIKE 'G')"#
    );
}

#[test]
fn select_23() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .and_where_option(None)
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character""#
    );
}

#[test]
fn select_24() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .conditions(
                true,
                |x| {
                    x.and_where(Expr::col(Char::FontId).eq(5));
                },
                |_| ()
            )
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" WHERE "font_id" = 5"#
    );
}

#[test]
fn select_25() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .and_where(
                Expr::col(Char::SizeW)
                    .mul(2)
                    .eq(Expr::col(Char::SizeH).div(2))
            )
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" WHERE "size_w" * 2 = "size_h" / 2"#
    );
}

#[test]
fn select_26() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .and_where(
                Expr::expr(Expr::col(Char::SizeW).add(1))
                    .mul(2)
                    .eq(Expr::expr(Expr::col(Char::SizeH).div(2)).sub(1))
            )
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" WHERE ("size_w" + 1) * 2 = ("size_h" / 2) - 1"#
    );
}

#[test]
fn select_27() {
    assert_eq!(
        Query::select()
            .columns([Char::Character, Char::SizeW, Char::SizeH])
            .from(Char::Table)
            .and_where(Expr::col(Char::SizeW).eq(3))
            .and_where(Expr::col(Char::SizeH).eq(4))
            .and_where(Expr::col(Char::SizeH).eq(5))
            .to_string(QueryBuilder),
        r#"SELECT "character", "size_w", "size_h" FROM "character" WHERE "size_w" = 3 AND "size_h" = 4 AND "size_h" = 5"#
    );
}

#[test]
fn select_28() {
    assert_eq!(
        Query::select()
            .columns([Char::Character, Char::SizeW, Char::SizeH])
            .from(Char::Table)
            .cond_where(any![
                Expr::col(Char::SizeW).eq(3),
                Expr::col(Char::SizeH).eq(4),
                Expr::col(Char::SizeH).eq(5),
            ])
            .to_string(QueryBuilder),
        r#"SELECT "character", "size_w", "size_h" FROM "character" WHERE "size_w" = 3 OR "size_h" = 4 OR "size_h" = 5"#
    );
}

#[test]
fn select_30() {
    assert_eq!(
        Query::select()
            .columns([Char::Character, Char::SizeW, Char::SizeH])
            .from(Char::Table)
            .and_where(
                Expr::col(Char::SizeW)
                    .mul(2)
                    .add(Expr::col(Char::SizeH).div(3))
                    .eq(4)
            )
            .to_string(QueryBuilder),
        r#"SELECT "character", "size_w", "size_h" FROM "character" WHERE ("size_w" * 2) + ("size_h" / 3) = 4"#
    );
}

#[test]
fn select_31() {
    assert_eq!(
        Query::select()
            .expr((1..10_i32).fold(Expr::value(0), |expr, i| { expr.add(i) }))
            .to_string(QueryBuilder),
        r#"SELECT 0 + 1 + 2 + 3 + 4 + 5 + 6 + 7 + 8 + 9"#
    );
}

#[test]
fn select_32() {
    assert_eq!(
        Query::select()
            .expr_as(Expr::col(Char::Character), Alias::new("C"))
            .from(Char::Table)
            .to_string(QueryBuilder),
        r#"SELECT "character" AS "C" FROM "character""#
    );
}

#[test]
fn select_33() {
    assert_eq!(
        Query::select()
            .column(Glyph::Image)
            .from(Glyph::Table)
            .and_where(
                Expr::col(Glyph::Aspect)
                    .in_subquery(Query::select().expr(Expr::cust("3 + 2 * 2")).take())
            )
            .to_string(QueryBuilder),
        r#"SELECT "image" FROM "glyph" WHERE "aspect" IN (SELECT 3 + 2 * 2)"#
    );
}

#[test]
fn select_34a() {
    assert_eq!(
        Query::select()
            .column(Glyph::Aspect)
            .expr(Expr::col(Glyph::Image).max())
            .from(Glyph::Table)
            .group_by_columns([Glyph::Aspect])
            .cond_having(any![
                Expr::col(Glyph::Aspect)
                    .gt(2)
                    .or(Expr::col(Glyph::Aspect).lt(8)),
                Expr::col(Glyph::Aspect)
                    .gt(12)
                    .and(Expr::col(Glyph::Aspect).lt(18)),
                Expr::col(Glyph::Aspect).gt(32),
            ])
            .to_string(QueryBuilder),
        [
            r#"SELECT "aspect", MAX("image") FROM "glyph" GROUP BY "aspect""#,
            r#"HAVING "aspect" > 2 OR "aspect" < 8"#,
            r#"OR ("aspect" > 12 AND "aspect" < 18)"#,
            r#"OR "aspect" > 32"#,
        ]
        .join(" ")
    );
}

#[test]
fn select_35() {
    let (statement, values) = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .and_where(Expr::col(Glyph::Aspect).is_null())
        .build(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "id" FROM "glyph" WHERE "aspect" IS NULL"#
    );
    assert_eq!(values.0, vec![]);
}

#[test]
fn select_36() {
    let (statement, values) = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(Cond::any().add(Expr::col(Glyph::Aspect).is_null()))
        .build(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "id" FROM "glyph" WHERE "aspect" IS NULL"#
    );
    assert_eq!(values.0, vec![]);
}

// [spec:pgorm:sem:sql.ast.condition.flattening/test]
#[test]
fn select_37() {
    let (statement, values) = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(Cond::any().add(Cond::all()).add(Cond::any()))
        .build(QueryBuilder);

    assert_eq!(statement, r#"SELECT "id" FROM "glyph" WHERE TRUE OR FALSE"#);
    assert_eq!(values.0, vec![]);
}

#[test]
fn select_37a() {
    let (statement, values) = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(
            Cond::all()
                .add(Cond::all().not())
                .add(Cond::any().not())
                .not(),
        )
        .build(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "id" FROM "glyph" WHERE NOT ((NOT TRUE) AND (NOT FALSE))"#
    );
    assert_eq!(values.0, vec![]);
}

#[test]
fn select_38() {
    let (statement, values) = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(
            Cond::any()
                .add(Expr::col(Glyph::Aspect).is_null())
                .add(Expr::col(Glyph::Aspect).is_not_null()),
        )
        .build(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "id" FROM "glyph" WHERE "aspect" IS NULL OR "aspect" IS NOT NULL"#
    );
    assert_eq!(values.0, vec![]);
}

#[test]
fn select_39() {
    let (statement, values) = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(
            Cond::all()
                .add(Expr::col(Glyph::Aspect).is_null())
                .add(Expr::col(Glyph::Aspect).is_not_null()),
        )
        .build(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "id" FROM "glyph" WHERE "aspect" IS NULL AND "aspect" IS NOT NULL"#
    );
    assert_eq!(values.0, vec![]);
}

#[test]
fn select_40() {
    let statement = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(any![
            Expr::col(Glyph::Aspect).is_null(),
            all![
                Expr::col(Glyph::Aspect).is_not_null(),
                Expr::col(Glyph::Aspect).lt(8)
            ]
        ])
        .to_string(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "id" FROM "glyph" WHERE "aspect" IS NULL OR ("aspect" IS NOT NULL AND "aspect" < 8)"#
    );
}

#[test]
fn select_41() {
    assert_eq!(
        Query::select()
            .columns([Glyph::Aspect])
            .exprs([Expr::col(Glyph::Image).max()])
            .from(Glyph::Table)
            .group_by_columns([Glyph::Aspect])
            .cond_having(any![Expr::col(Glyph::Aspect).gt(2)])
            .to_string(QueryBuilder),
        r#"SELECT "aspect", MAX("image") FROM "glyph" GROUP BY "aspect" HAVING "aspect" > 2"#
    );
}

#[test]
fn select_42() {
    let statement = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(
            Cond::all()
                .add_option(Some(Expr::col(Glyph::Aspect).lt(8)))
                .add(Expr::col(Glyph::Aspect).is_not_null()),
        )
        .to_string(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "id" FROM "glyph" WHERE "aspect" < 8 AND "aspect" IS NOT NULL"#
    );
}

#[test]
fn select_43() {
    let statement = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(Cond::all().add_option::<SimpleExpr>(None))
        .to_string(QueryBuilder);

    assert_eq!(statement, r#"SELECT "id" FROM "glyph" WHERE TRUE"#);
}

#[test]
fn select_44() {
    let statement = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(
            Cond::any()
                .not()
                .add_option(Some(Expr::col(Glyph::Aspect).lt(8))),
        )
        .to_string(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "id" FROM "glyph" WHERE NOT "aspect" < 8"#
    );
}

#[test]
fn select_45() {
    let statement = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(
            Cond::any()
                .not()
                .add_option(Some(Expr::col(Glyph::Aspect).lt(8)))
                .add(Expr::col(Glyph::Aspect).is_not_null()),
        )
        .to_string(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "id" FROM "glyph" WHERE NOT ("aspect" < 8 OR "aspect" IS NOT NULL)"#
    );
}

#[test]
fn select_46() {
    let statement = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(
            Cond::all()
                .not()
                .add_option(Some(Expr::col(Glyph::Aspect).lt(8))),
        )
        .to_string(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "id" FROM "glyph" WHERE NOT "aspect" < 8"#
    );
}

#[test]
fn select_47() {
    let statement = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(
            Cond::all()
                .not()
                .add_option(Some(Expr::col(Glyph::Aspect).lt(8)))
                .add(Expr::col(Glyph::Aspect).is_not_null()),
        )
        .to_string(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "id" FROM "glyph" WHERE NOT ("aspect" < 8 AND "aspect" IS NOT NULL)"#
    );
}

#[test]
fn select_48() {
    let statement = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(
            Cond::all().add_option(Some(ConditionExpression::SimpleExpr(
                Expr::tuple([Expr::col(Glyph::Aspect).into(), Expr::value(100)])
                    .lt(Expr::tuple([Expr::value(8), Expr::value(100)])),
            ))),
        )
        .to_string(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "id" FROM "glyph" WHERE ("aspect", 100) < (8, 100)"#
    );
}

#[test]
fn select_48a() {
    let statement = Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(
            Cond::all().add_option(Some(ConditionExpression::SimpleExpr(
                Expr::tuple([
                    Expr::col(Glyph::Aspect).into(),
                    Expr::value(String::from("100")),
                ])
                .in_tuples([(8, String::from("100"))]),
            ))),
        )
        .to_string(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "id" FROM "glyph" WHERE ("aspect", '100') IN ((8, '100'))"#
    );
}

// [spec:pgorm:def:sql.ast.keywords/test]    `Asterisk` as a bare projection
#[test]
fn select_49() {
    let statement = Query::select()
        .column(Asterisk)
        .from(Char::Table)
        .to_string(QueryBuilder);

    assert_eq!(statement, r#"SELECT * FROM "character""#);
}

// [spec:pgorm:def:sql.ast.keywords/test]    `(Table, Asterisk)` renders `"table".*`
#[test]
fn select_50() {
    let statement = Query::select()
        .column((Char::Table, Asterisk))
        .column((Font::Table, Font::Name))
        .from(Char::Table)
        .inner_join(
            Font::Table,
            Expr::col((Char::Table, Char::FontId)).equals((Font::Table, Font::Id)),
        )
        .to_string(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "character".*, "font"."name" FROM "character" INNER JOIN "font" ON "character"."font_id" = "font"."id""#
    )
}

// [spec:pgorm:req:sql.ast.order/test]
#[test]
fn select_51() {
    assert_eq!(
        Query::select()
            .columns([Glyph::Aspect])
            .from(Glyph::Table)
            .and_where(Expr::expr(Expr::col(Glyph::Aspect).if_null(0)).gt(2))
            .order_by_with_nulls(Glyph::Image, Order::Desc, NullOrdering::First)
            .order_by_with_nulls(
                (Glyph::Table, Glyph::Aspect),
                Order::Asc,
                NullOrdering::Last
            )
            .to_string(QueryBuilder),
        [
            r#"SELECT "aspect""#,
            r#"FROM "glyph""#,
            r#"WHERE COALESCE("aspect", 0) > 2"#,
            r#"ORDER BY "image" DESC NULLS FIRST,"#,
            r#""glyph"."aspect" ASC NULLS LAST"#,
        ]
        .join(" ")
    );
}

#[test]
fn select_52() {
    assert_eq!(
        Query::select()
            .columns([Glyph::Aspect])
            .from(Glyph::Table)
            .and_where(Expr::expr(Expr::col(Glyph::Aspect).if_null(0)).gt(2))
            .order_by_columns_with_nulls([
                (Glyph::Id, Order::Asc, NullOrdering::First),
                (Glyph::Aspect, Order::Desc, NullOrdering::Last),
            ])
            .to_string(QueryBuilder),
        [
            r#"SELECT "aspect""#,
            r#"FROM "glyph""#,
            r#"WHERE COALESCE("aspect", 0) > 2"#,
            r#"ORDER BY "id" ASC NULLS FIRST,"#,
            r#""aspect" DESC NULLS LAST"#,
        ]
        .join(" ")
    );
}

#[test]
fn select_53() {
    assert_eq!(
        Query::select()
            .columns([Glyph::Aspect])
            .from(Glyph::Table)
            .and_where(Expr::expr(Expr::col(Glyph::Aspect).if_null(0)).gt(2))
            .order_by_columns_with_nulls([
                ((Glyph::Table, Glyph::Id), Order::Asc, NullOrdering::First),
                (
                    (Glyph::Table, Glyph::Aspect),
                    Order::Desc,
                    NullOrdering::Last
                ),
            ])
            .to_string(QueryBuilder),
        [
            r#"SELECT "aspect""#,
            r#"FROM "glyph""#,
            r#"WHERE COALESCE("aspect", 0) > 2"#,
            r#"ORDER BY "glyph"."id" ASC NULLS FIRST,"#,
            r#""glyph"."aspect" DESC NULLS LAST"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:sql.ast.select.projection+1/test]
#[test]
fn select_54() {
    assert_eq!(
        Query::select()
            .distinct_on([Glyph::Aspect])
            .columns([Glyph::Aspect])
            .from(Glyph::Table)
            .and_where(Expr::expr(Expr::col(Glyph::Aspect).if_null(0)).gt(2))
            .order_by_columns_with_nulls([
                ((Glyph::Table, Glyph::Id), Order::Asc, NullOrdering::First),
                (
                    (Glyph::Table, Glyph::Aspect),
                    Order::Desc,
                    NullOrdering::Last
                ),
            ])
            .to_string(QueryBuilder),
        [
            r#"SELECT DISTINCT ON ("aspect") "aspect""#,
            r#"FROM "glyph""#,
            r#"WHERE COALESCE("aspect", 0) > 2"#,
            r#"ORDER BY "glyph"."id" ASC NULLS FIRST,"#,
            r#""glyph"."aspect" DESC NULLS LAST"#,
        ]
        .join(" ")
    );
}

#[test]
fn select_55() {
    let statement = Query::select()
        .column(Asterisk)
        .from(Char::Table)
        .from(Font::Table)
        .and_where(Expr::col((Font::Table, Font::Id)).equals((Char::Table, Char::FontId)))
        .to_string(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT * FROM "character", "font" WHERE "font"."id" = "character"."font_id""#
    );
}

// [spec:pgorm:req:sql.render.select-order/test]
#[test]
fn select_56() {
    assert_eq!(
        Query::select()
            .columns([Glyph::Aspect])
            .from(Glyph::Table)
            .and_where(Expr::expr(Expr::col(Glyph::Aspect).if_null(0)).gt(2))
            .order_by(
                Glyph::Id,
                Order::Field(Values(vec![
                    Value::Int(Some(4)),
                    Value::Int(Some(5)),
                    Value::Int(Some(1)),
                    Value::Int(Some(3))
                ]))
            )
            .order_by((Glyph::Table, Glyph::Aspect), Order::Asc)
            .to_string(QueryBuilder),
        [
            r#"SELECT "aspect""#,
            r#"FROM "glyph""#,
            r#"WHERE COALESCE("aspect", 0) > 2"#,
            r#"ORDER BY CASE"#,
            r#"WHEN "id"=4 THEN 0"#,
            r#"WHEN "id"=5 THEN 1"#,
            r#"WHEN "id"=1 THEN 2"#,
            r#"WHEN "id"=3 THEN 3"#,
            r#"ELSE 4 END,"#,
            r#""glyph"."aspect" ASC"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:sql.ast.order/test]
#[test]
fn select_57() {
    assert_eq!(
        Query::select()
            .columns([Glyph::Aspect])
            .from(Glyph::Table)
            .and_where(Expr::expr(Expr::col(Glyph::Aspect).if_null(0)).gt(2))
            .order_by((Glyph::Table, Glyph::Aspect), Order::Asc)
            .order_by(
                Glyph::Id,
                Order::Field(Values(vec![
                    Value::Int(Some(4)),
                    Value::Int(Some(5)),
                    Value::Int(Some(1)),
                    Value::Int(Some(3))
                ]))
            )
            .to_string(QueryBuilder),
        [
            r#"SELECT "aspect""#,
            r#"FROM "glyph""#,
            r#"WHERE COALESCE("aspect", 0) > 2"#,
            r#"ORDER BY "glyph"."aspect" ASC,"#,
            r#"CASE WHEN "id"=4 THEN 0"#,
            r#"WHEN "id"=5 THEN 1"#,
            r#"WHEN "id"=1 THEN 2"#,
            r#"WHEN "id"=3 THEN 3"#,
            r#"ELSE 4 END"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:def:sql.ast.with/test]
// [spec:pgorm:req:sql.render.cte/test]
#[test]
fn select_58() {
    let select = SelectStatement::new()
        .columns([Glyph::Id, Glyph::Image, Glyph::Aspect])
        .from(Glyph::Table)
        .to_owned();
    let cte = CommonTableExpression::new()
        .query(select)
        .table_name(Alias::new("cte"))
        .to_owned();
    let with_clause = WithClause::new().cte(cte).to_owned();
    let select = SelectStatement::new()
        .columns([Glyph::Id, Glyph::Image, Glyph::Aspect])
        .from(Alias::new("cte"))
        .to_owned();
    assert_eq!(
        select.with(with_clause).to_string(QueryBuilder),
        [
            r#"WITH "cte" AS"#,
            r#"(SELECT "id", "image", "aspect""#,
            r#"FROM "glyph")"#,
            r#"SELECT "id", "image", "aspect" FROM "cte""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:def:sql.ast.case/test]
#[test]
fn select_59() {
    let query = Query::select()
        .expr_as(
            CaseStatement::new()
                .case(Expr::col((Glyph::Table, Glyph::Aspect)).gt(0), "positive")
                .case(Expr::col((Glyph::Table, Glyph::Aspect)).lt(0), "negative")
                .finally("zero"),
            Alias::new("polarity"),
        )
        .from(Glyph::Table)
        .to_owned();

    assert_eq!(
        query.to_string(QueryBuilder),
        r#"SELECT (CASE WHEN ("glyph"."aspect" > 0) THEN 'positive' WHEN ("glyph"."aspect" < 0) THEN 'negative' ELSE 'zero' END) AS "polarity" FROM "glyph""#
    );
}

// [spec:pgorm:req:sql.ast.build/test]
// [spec:pgorm:req:sql.render.placeholders/test]
// [spec:pgorm:req:sql.render.param-vs-inline/test]
// [spec:pgorm:req:sql.render.custom-expr/test]
#[test]
fn select_60() {
    let (cust_query, cust_values) = Query::select()
        .column(Character::Id)
        .from(Character::Table)
        .and_where(Expr::col(Character::FontSize).eq(3))
        .build(QueryBuilder);

    let (statement, values) = Query::select()
        .expr(Expr::cust_with_values(&cust_query[7..], cust_values.0))
        .limit(5)
        .build(QueryBuilder);

    assert_eq!(
        statement,
        r#"SELECT "id" FROM "character" WHERE "font_size" = $1 LIMIT $2"#
    );
    assert_eq!(values, Values(vec![3i32.into(), 5u64.into()]));
}

// [spec:pgorm:req:sql.ast.expr.operators/test]
#[test]
fn select_61() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .and_where(Expr::col(Char::Character).like(LikeExpr::new("A").escape('\\')))
            .build(QueryBuilder),
        (
            r#"SELECT "character" FROM "character" WHERE "character" LIKE $1 ESCAPE E'\\'"#
                .to_owned(),
            Values(vec!["A".into()])
        )
    );
}

// [spec:pgorm:req:sql.ast.select.from/test]
// [spec:pgorm:req:sql.render.subquery/test]
#[test]
fn select_62() {
    let select = SelectStatement::new()
        .column(Asterisk)
        .from_values([(1i32, "hello"), (2, "world")], Alias::new("x"))
        .to_owned();
    let cte = CommonTableExpression::new()
        .query(select)
        .table_name(Alias::new("cte"))
        .to_owned();
    let with_clause = WithClause::new().cte(cte).to_owned();
    let select = SelectStatement::new()
        .columns([Alias::new("column1"), Alias::new("column2")])
        .from(Alias::new("cte"))
        .to_owned();
    assert_eq!(
        select.with(with_clause).to_string(QueryBuilder),
        [
            r#"WITH "cte" AS"#,
            r#"(SELECT * FROM (VALUES (1, 'hello'), (2, 'world')) AS "x")"#,
            r#"SELECT "column1", "column2""#,
            r#"FROM "cte""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:def:sql.ast.insert/test]
// [spec:pgorm:req:sql.render.insert/test]
// [spec:pgorm:def:sql.render.value-literals/test]
#[test]
#[allow(clippy::approx_constant)]
fn insert_2() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Image, Glyph::Aspect])
            .values_panic([
                "04108048005887010020060000204E0180400400".into(),
                3.1415.into(),
            ])
            .to_string(QueryBuilder),
        r#"INSERT INTO "glyph" ("image", "aspect") VALUES ('04108048005887010020060000204E0180400400', 3.1415)"#
    );
}

#[test]
#[allow(clippy::approx_constant)]
fn insert_3() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Image, Glyph::Aspect])
            .values_panic([
                "04108048005887010020060000204E0180400400".into(),
                3.1415.into(),
            ])
            .values_panic([Value::String(None).into(), 2.1345.into()])
            .to_string(QueryBuilder),
        r#"INSERT INTO "glyph" ("image", "aspect") VALUES ('04108048005887010020060000204E0180400400', 3.1415), (NULL, 2.1345)"#
    );
}

#[test]

fn insert_4() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Image])
            .values_panic([chrono::DateTime::from_timestamp(0, 0)
                .unwrap()
                .naive_utc()
                .into()])
            .to_string(QueryBuilder),
        "INSERT INTO \"glyph\" (\"image\") VALUES ('1970-01-01 00:00:00')"
    );
}

#[test]

fn insert_5() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Image])
            .values_panic([uuid::Uuid::nil().into()])
            .to_string(QueryBuilder),
        "INSERT INTO \"glyph\" (\"image\") VALUES ('00000000-0000-0000-0000-000000000000')"
    );
}

#[test]
fn insert_from_select() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .or_default_values()
            .columns([Glyph::Aspect, Glyph::Image])
            .select_from(
                Query::select()
                    .column(Glyph::Aspect)
                    .column(Glyph::Image)
                    .from(Glyph::Table)
                    .conditions(
                        true,
                        |x| {
                            x.and_where(Expr::col(Glyph::Image).like("%"));
                        },
                        |x| {
                            x.and_where(Expr::col(Glyph::Id).eq(6));
                        },
                    )
                    .to_owned()
            )
            .unwrap()
            .to_owned()
            .to_string(QueryBuilder),
        r#"INSERT INTO "glyph" ("aspect", "image") SELECT "aspect", "image" FROM "glyph" WHERE "image" LIKE '%'"#
    );
}

// [spec:pgorm:def:sql.ast.with/test]
#[test]
fn insert_6() -> error::Result<()> {
    let select = SelectStatement::new()
        .columns([Glyph::Id, Glyph::Image, Glyph::Aspect])
        .from(Glyph::Table)
        .to_owned();
    let cte = CommonTableExpression::new()
        .query(select)
        .column(Glyph::Id)
        .column(Glyph::Image)
        .column(Glyph::Aspect)
        .table_name(Alias::new("cte"))
        .to_owned();
    let with_clause = WithClause::new().cte(cte).to_owned();
    let select = SelectStatement::new()
        .columns([Glyph::Id, Glyph::Image, Glyph::Aspect])
        .from(Alias::new("cte"))
        .to_owned();
    let mut insert = Query::insert();
    insert
        .into_table(Glyph::Table)
        .columns([Glyph::Id, Glyph::Image, Glyph::Aspect])
        .select_from(select)?;
    let sql = insert.with(with_clause).to_string(QueryBuilder);
    assert_eq!(
        sql.as_str(),
        [
            r#"WITH "cte" ("id", "image", "aspect") AS (SELECT "id", "image", "aspect" FROM "glyph")"#,
            r#"INSERT INTO "glyph" ("id", "image", "aspect") SELECT "id", "image", "aspect" FROM "cte""#,
        ].join(" ")
    );
    Ok(())
}

#[test]
fn insert_7() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .or_default_values()
            .to_string(QueryBuilder),
        r#"INSERT INTO "glyph" VALUES (DEFAULT)"#
    );
}

#[test]
fn insert_8() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .or_default_values()
            .returning_col(Glyph::Id)
            .to_string(QueryBuilder),
        r#"INSERT INTO "glyph" VALUES (DEFAULT) RETURNING "id""#
    );
}

#[test]
#[allow(clippy::approx_constant)]
fn insert_10() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect, Glyph::Tokens])
            .values_panic([
                3.1415.into(),
                vec![
                    "Token1".to_string(),
                    "Token2".to_string(),
                    "Token3".to_string()
                ]
                .into()
            ])
            .to_string(QueryBuilder),
        r#"INSERT INTO "glyph" ("aspect", "tokens") VALUES (3.1415, ARRAY ['Token1','Token2','Token3'])"#
    );
}

// [spec:pgorm:req:sql.ast.on-conflict/test]
#[test]
#[allow(clippy::approx_constant)]
// [spec:pgorm:req:sql.render.on-conflict/test]
fn insert_on_conflict_1() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect, Glyph::Image])
            .values_panic([
                "04108048005887010020060000204E0180400400".into(),
                3.1415.into(),
            ])
            .on_conflict(
                OnConflict::column(Glyph::Id)
                    .update_column(Glyph::Aspect)
                    .to_owned()
            )
            .to_string(QueryBuilder),
        [
            r#"INSERT INTO "glyph" ("aspect", "image")"#,
            r#"VALUES ('04108048005887010020060000204E0180400400', 3.1415)"#,
            r#"ON CONFLICT ("id") DO UPDATE SET "aspect" = "excluded"."aspect""#,
        ]
        .join(" ")
    );
}

#[test]
#[allow(clippy::approx_constant)]
fn insert_on_conflict_2() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect, Glyph::Image])
            .values_panic([
                "04108048005887010020060000204E0180400400".into(),
                3.1415.into(),
            ])
            .on_conflict(
                OnConflict::columns([Glyph::Id, Glyph::Aspect])
                    .update_columns([Glyph::Aspect, Glyph::Image])
                    .to_owned()
            )
            .to_string(QueryBuilder),
        [
            r#"INSERT INTO "glyph" ("aspect", "image")"#,
            r#"VALUES ('04108048005887010020060000204E0180400400', 3.1415)"#,
            r#"ON CONFLICT ("id", "aspect") DO UPDATE SET "aspect" = "excluded"."aspect", "image" = "excluded"."image""#,
        ]
        .join(" ")
    );
}

#[test]
#[allow(clippy::approx_constant)]
fn insert_on_conflict_3() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect, Glyph::Image])
            .values_panic([
                "04108048005887010020060000204E0180400400".into(),
                3.1415.into(),
            ])
            .on_conflict(
                OnConflict::columns([Glyph::Id, Glyph::Aspect])
                    .values([
                        (Glyph::Aspect, "04108048005887010020060000204E0180400400".into()),
                        (Glyph::Image, 3.1415.into()),
                    ])
                    .to_owned()
            )
            .to_string(QueryBuilder),
        [
            r#"INSERT INTO "glyph" ("aspect", "image")"#,
            r#"VALUES ('04108048005887010020060000204E0180400400', 3.1415)"#,
            r#"ON CONFLICT ("id", "aspect") DO UPDATE SET "aspect" = '04108048005887010020060000204E0180400400', "image" = 3.1415"#,
        ]
        .join(" ")
    );
}

#[test]
#[allow(clippy::approx_constant)]
fn insert_on_conflict_4() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect, Glyph::Image])
            .values_panic([
                "04108048005887010020060000204E0180400400".into(),
                3.1415.into(),
            ])
            .on_conflict(
                OnConflict::columns([Glyph::Id, Glyph::Aspect])
                    .value(Glyph::Image, Expr::val(1).add(2))
                    .to_owned()
            )
            .to_string(QueryBuilder),
        [
            r#"INSERT INTO "glyph" ("aspect", "image")"#,
            r#"VALUES ('04108048005887010020060000204E0180400400', 3.1415)"#,
            r#"ON CONFLICT ("id", "aspect") DO UPDATE SET "image" = 1 + 2"#,
        ]
        .join(" ")
    );
}

#[test]
#[allow(clippy::approx_constant)]
fn insert_on_conflict_5() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect, Glyph::Image])
            .values_panic([
                "04108048005887010020060000204E0180400400".into(),
                3.1415.into(),
            ])
            .on_conflict(
                OnConflict::columns([Glyph::Id, Glyph::Aspect])
                    .value(Glyph::Aspect, Expr::val("04108048005887010020060000204E0180400400"))
                    .update_column(Glyph::Image)
                    .to_owned()
            )
            .to_string(QueryBuilder),
        [
            r#"INSERT INTO "glyph" ("aspect", "image")"#,
            r#"VALUES ('04108048005887010020060000204E0180400400', 3.1415)"#,
            r#"ON CONFLICT ("id", "aspect") DO UPDATE SET "aspect" = '04108048005887010020060000204E0180400400', "image" = "excluded"."image""#,
        ]
        .join(" ")
    );
}

#[test]
#[allow(clippy::approx_constant)]
fn insert_on_conflict_6() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect, Glyph::Image])
            .values_panic([
                "04108048005887010020060000204E0180400400".into(),
                3.1415.into(),
            ])
            .on_conflict(
                OnConflict::columns([Glyph::Id, Glyph::Aspect])
                    .update_column(Glyph::Aspect)
                    .value(Glyph::Image, Expr::val(1).add(2))
                    .to_owned()
            )
            .to_string(QueryBuilder),
        [
            r#"INSERT INTO "glyph" ("aspect", "image")"#,
            r#"VALUES ('04108048005887010020060000204E0180400400', 3.1415)"#,
            r#"ON CONFLICT ("id", "aspect") DO UPDATE SET "aspect" = "excluded"."aspect", "image" = 1 + 2"#,
        ]
        .join(" ")
    );
}

#[test]
#[allow(clippy::approx_constant)]
fn insert_on_conflict_7() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect, Glyph::Image])
            .values_panic([
                "04108048005887010020060000204E0180400400".into(),
                3.1415.into(),
            ])
            .on_conflict(
                OnConflict::new()
                    .expr(Expr::col(Glyph::Id))
                    .update_column(Glyph::Aspect)
                    .to_owned()
            )
            .to_string(QueryBuilder),
        [
            r#"INSERT INTO "glyph" ("aspect", "image")"#,
            r#"VALUES ('04108048005887010020060000204E0180400400', 3.1415)"#,
            r#"ON CONFLICT ("id") DO UPDATE SET "aspect" = "excluded"."aspect""#,
        ]
        .join(" ")
    );
}

#[test]
#[allow(clippy::approx_constant)]
fn insert_on_conflict_8() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect, Glyph::Image])
            .values_panic([
                "04108048005887010020060000204E0180400400".into(),
                3.1415.into(),
            ])
            .on_conflict(
                OnConflict::new()
                    .exprs([Expr::col(Glyph::Id), Expr::col(Glyph::Aspect)])
                    .update_column(Glyph::Aspect)
                    .to_owned()
            )
            .to_string(QueryBuilder),
        [
            r#"INSERT INTO "glyph" ("aspect", "image")"#,
            r#"VALUES ('04108048005887010020060000204E0180400400', 3.1415)"#,
            r#"ON CONFLICT ("id", "aspect") DO UPDATE SET "aspect" = "excluded"."aspect""#,
        ]
        .join(" ")
    );
}

#[test]
#[allow(clippy::approx_constant)]
fn insert_on_conflict_9() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect, Glyph::Image])
            .values_panic([
                "04108048005887010020060000204E0180400400".into(),
                3.1415.into(),
            ])
            .on_conflict(
                OnConflict::column(Glyph::Id)
                    .expr(Func::lower(Expr::col(Glyph::Tokens)))
                    .update_column(Glyph::Aspect)
                    .to_owned()
            )
            .to_string(QueryBuilder),
        [
            r#"INSERT INTO "glyph" ("aspect", "image")"#,
            r#"VALUES ('04108048005887010020060000204E0180400400', 3.1415)"#,
            r#"ON CONFLICT ("id", LOWER("tokens")) DO UPDATE SET "aspect" = "excluded"."aspect""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:sql.ast.on-conflict/test]
#[test]
#[allow(clippy::approx_constant)]
fn insert_on_conflict_do_nothing() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect, Glyph::Image])
            .values_panic(["abcd".into(), 3.1415.into()])
            .on_conflict(
                OnConflict::columns([Glyph::Id, Glyph::Aspect])
                    .do_nothing()
                    .to_owned(),
            )
            .to_string(QueryBuilder),
        [
            r#"INSERT INTO "glyph" ("aspect", "image")"#,
            r#"VALUES ('abcd', 3.1415)"#,
            r#"ON CONFLICT ("id", "aspect") DO NOTHING"#,
        ]
        .join(" ")
    );
}

#[test]
#[allow(clippy::approx_constant)]
fn insert_on_conflict_do_nothing_on() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Aspect, Glyph::Image])
            .values_panic(["abcd".into(), 3.1415.into()])
            .on_conflict(
                OnConflict::columns([Glyph::Id, Glyph::Aspect])
                    .do_nothing_on([Glyph::Id])
                    .to_owned(),
            )
            .to_string(QueryBuilder),
        [
            r#"INSERT INTO "glyph" ("aspect", "image")"#,
            r#"VALUES ('abcd', 3.1415)"#,
            r#"ON CONFLICT ("id", "aspect") DO NOTHING"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:def:sql.ast.returning/test]
#[test]
#[allow(clippy::approx_constant)]
// [spec:pgorm:req:sql.render.returning/test]
fn insert_returning_all_columns() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Image, Glyph::Aspect])
            .values_panic([
                "04108048005887010020060000204E0180400400".into(),
                3.1415.into(),
            ])
            .returning(Query::returning().all())
            .to_string(QueryBuilder),
        r#"INSERT INTO "glyph" ("image", "aspect") VALUES ('04108048005887010020060000204E0180400400', 3.1415) RETURNING *"#
    );
}

#[test]
#[allow(clippy::approx_constant)]
fn insert_returning_specific_columns() {
    assert_eq!(
        Query::insert()
            .into_table(Glyph::Table)
            .columns([Glyph::Image, Glyph::Aspect])
            .values_panic([
                "04108048005887010020060000204E0180400400".into(),
                3.1415.into(),
            ])
            .returning(Query::returning().columns([Glyph::Id, Glyph::Image]))
            .to_string(QueryBuilder),
        r#"INSERT INTO "glyph" ("image", "aspect") VALUES ('04108048005887010020060000204E0180400400', 3.1415) RETURNING "id", "image""#
    );
}

// [spec:pgorm:req:sql.ast.update/test]
#[test]
// [spec:pgorm:req:sql.render.update-delete/test]
fn update_1() {
    assert_eq!(
        Query::update()
            .table(Glyph::Table)
            .values([
                (Glyph::Aspect, 2.1345.into()),
                (
                    Glyph::Image,
                    "24B0E11951B03B07F8300FD003983F03F0780060".into()
                ),
            ])
            .and_where(Expr::col(Glyph::Id).eq(1))
            .to_string(QueryBuilder),
        r#"UPDATE "glyph" SET "aspect" = 2.1345, "image" = '24B0E11951B03B07F8300FD003983F03F0780060' WHERE "id" = 1"#
    );
}

#[test]
fn update_3() {
    assert_eq!(
        Query::update()
            .table(Glyph::Table)
            .value(Glyph::Aspect, Expr::cust("60 * 24 * 24"))
            .values([(
                Glyph::Image,
                "24B0E11951B03B07F8300FD003983F03F0780060".into()
            )])
            .and_where(Expr::col(Glyph::Id).eq(1))
            .to_string(QueryBuilder),
        r#"UPDATE "glyph" SET "aspect" = 60 * 24 * 24, "image" = '24B0E11951B03B07F8300FD003983F03F0780060' WHERE "id" = 1"#
    );
}

#[test]
fn update_4() {
    assert_eq_unparsed!(
        Query::update()
            .table(Glyph::Table)
            .value(Glyph::Aspect, Expr::col(Glyph::Aspect).add(1))
            .values([(
                Glyph::Image,
                "24B0E11951B03B07F8300FD003983F03F0780060".into()
            )])
            .and_where(Expr::col(Glyph::Id).eq(1))
            .order_by(Glyph::Id, Order::Asc)
            .limit(1)
            .to_string(QueryBuilder),
        r#"UPDATE "glyph" SET "aspect" = "aspect" + 1, "image" = '24B0E11951B03B07F8300FD003983F03F0780060' WHERE "id" = 1 ORDER BY "id" ASC LIMIT 1"#
    );
}

#[test]
fn update_returning_all_columns() {
    assert_eq!(
        Query::update()
            .table(Glyph::Table)
            .value(Glyph::Aspect, Expr::cust("60 * 24 * 24"))
            .values([(
                Glyph::Image,
                "24B0E11951B03B07F8300FD003983F03F0780060".into()
            )])
            .and_where(Expr::col(Glyph::Id).eq(1))
            .returning(Query::returning().all())
            .to_string(QueryBuilder),
        r#"UPDATE "glyph" SET "aspect" = 60 * 24 * 24, "image" = '24B0E11951B03B07F8300FD003983F03F0780060' WHERE "id" = 1 RETURNING *"#
    );
}

#[test]
fn update_returning_specified_columns() {
    assert_eq!(
        Query::update()
            .table(Glyph::Table)
            .value(Glyph::Aspect, Expr::cust("60 * 24 * 24"))
            .values([(
                Glyph::Image,
                "24B0E11951B03B07F8300FD003983F03F0780060".into()
            )])
            .and_where(Expr::col(Glyph::Id).eq(1))
            .returning(Query::returning().columns([Glyph::Id, Glyph::Image]))
            .to_string(QueryBuilder),
        r#"UPDATE "glyph" SET "aspect" = 60 * 24 * 24, "image" = '24B0E11951B03B07F8300FD003983F03F0780060' WHERE "id" = 1 RETURNING "id", "image""#
    );
}

// [spec:pgorm:def:sql.ast.delete/test]
#[test]
// [spec:pgorm:req:sql.render.update-delete/test]
fn delete_1() {
    assert_eq!(
        Query::delete()
            .from_table(Glyph::Table)
            .and_where(Expr::col(Glyph::Id).eq(1))
            .to_string(QueryBuilder),
        r#"DELETE FROM "glyph" WHERE "id" = 1"#
    );
}

#[test]
// [spec:pgorm:req:sql.render.string-escape/test]
fn escape_1() {
    let test = r#" "abc" "#;
    assert_eq!(QueryBuilder.escape_string(test), r#" \"abc\" "#.to_owned());
    assert_eq!(
        QueryBuilder.unescape_string(QueryBuilder.escape_string(test).as_str()),
        test
    )
}

#[test]
fn escape_2() {
    let test = "a\nb\tc";
    assert_eq!(QueryBuilder.escape_string(test), "a\\nb\\tc".to_owned());
    assert_eq!(
        QueryBuilder.unescape_string(QueryBuilder.escape_string(test).as_str()),
        test
    );
}

#[test]
fn escape_3() {
    let test = "a\\b";
    assert_eq!(QueryBuilder.escape_string(test), "a\\\\b".to_owned());
    assert_eq!(
        QueryBuilder.unescape_string(QueryBuilder.escape_string(test).as_str()),
        test
    );
}

#[test]
fn escape_4() {
    let test = "a\"b";
    assert_eq!(QueryBuilder.escape_string(test), "a\\\"b".to_owned());
    assert_eq!(
        QueryBuilder.unescape_string(QueryBuilder.escape_string(test).as_str()),
        test
    )
}

#[test]
fn delete_returning_all_columns() {
    assert_eq!(
        Query::delete()
            .from_table(Glyph::Table)
            .and_where(Expr::col(Glyph::Id).eq(1))
            .returning(Query::returning().all())
            .to_string(QueryBuilder),
        r#"DELETE FROM "glyph" WHERE "id" = 1 RETURNING *"#
    );
}

#[test]
fn delete_returning_specific_columns() {
    assert_eq!(
        Query::delete()
            .from_table(Glyph::Table)
            .and_where(Expr::col(Glyph::Id).eq(1))
            .returning(Query::returning().columns([Glyph::Id, Glyph::Image]))
            .to_string(QueryBuilder),
        r#"DELETE FROM "glyph" WHERE "id" = 1 RETURNING "id", "image""#
    );
}

#[test]
fn delete_returning_specific_exprs() {
    assert_eq!(
        Query::delete()
            .from_table(Glyph::Table)
            .and_where(Expr::col(Glyph::Id).eq(1))
            .returning(Query::returning().exprs([Expr::col(Glyph::Id), Expr::col(Glyph::Image)]))
            .to_string(QueryBuilder),
        r#"DELETE FROM "glyph" WHERE "id" = 1 RETURNING "id", "image""#
    );
}

#[test]
// [spec:pgorm:def:sql.render.operators/test]
fn select_pgtrgm_similarity() {
    assert_eq!(
        Query::select()
            .expr(Expr::col(Font::Name).binary(BinOper::Similarity, Expr::value("serif")))
            .from(Font::Table)
            .to_string(QueryBuilder),
        r#"SELECT "name" % 'serif' FROM "font""#
    );
}

#[test]
fn select_pgtrgm_word_similarity() {
    assert_eq!(
        Query::select()
            .expr(Expr::col(Font::Name).binary(BinOper::WordSimilarity, Expr::value("serif")))
            .from(Font::Table)
            .to_string(QueryBuilder),
        r#"SELECT "name" <% 'serif' FROM "font""#
    );
}

#[test]
fn select_pgtrgm_strict_word_similarity() {
    assert_eq!(
        Query::select()
            .expr(Expr::col(Font::Name).binary(BinOper::StrictWordSimilarity, Expr::value("serif")))
            .from(Font::Table)
            .to_string(QueryBuilder),
        r#"SELECT "name" <<% 'serif' FROM "font""#
    );
}

#[test]
fn select_pgtrgm_similarity_distance() {
    assert_eq!(
        Query::select()
            .expr(Expr::col(Font::Name).binary(BinOper::SimilarityDistance, Expr::value("serif")))
            .from(Font::Table)
            .to_string(QueryBuilder),
        r#"SELECT "name" <-> 'serif' FROM "font""#
    );
}

#[test]
fn select_pgtrgm_word_similarity_distance() {
    assert_eq!(
        Query::select()
            .expr(
                Expr::col(Font::Name).binary(BinOper::WordSimilarityDistance, Expr::value("serif"))
            )
            .from(Font::Table)
            .to_string(QueryBuilder),
        r#"SELECT "name" <<-> 'serif' FROM "font""#
    );
}

#[test]
fn select_pgtrgm_strict_word_similarity_distance() {
    assert_eq!(
        Query::select()
            .expr(
                Expr::col(Font::Name)
                    .binary(BinOper::StrictWordSimilarityDistance, Expr::value("serif"))
            )
            .from(Font::Table)
            .to_string(QueryBuilder),
        r#"SELECT "name" <<<-> 'serif' FROM "font""#
    );
}

#[test]
fn select_custom_operator() {
    assert_eq!(
        Query::select()
            .expr(Expr::col(Font::Name).binary(BinOper::Custom("~*"), Expr::value("serif")))
            .from(Font::Table)
            .to_string(QueryBuilder),
        r#"SELECT "name" ~* 'serif' FROM "font""#
    );
    assert_eq!(
        Query::select()
            .expr(Expr::col(Font::Name).binary(BinOper::Custom("~"), Expr::value("serif")))
            .from(Font::Table)
            .to_string(QueryBuilder),
        r#"SELECT "name" ~ 'serif' FROM "font""#
    );
}

// [spec:pgorm:sem:sql.ast.select.union/test]
#[test]
fn union_1() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .union(
                UnionType::Distinct,
                Query::select()
                    .column(Char::Character)
                    .from(Char::Table)
                    .left_join(
                        Font::Table,
                        Expr::col((Char::Table, Char::FontId)).equals((Font::Table, Font::Id))
                    )
                    .order_by((Font::Table, Font::Id), Order::Asc)
                    .take()
            )
            .to_string(QueryBuilder),
        [
            r#"SELECT "character" FROM "character" UNION (SELECT "character" FROM "character""#,
            r#"LEFT JOIN "font" ON "character"."font_id" = "font"."id" ORDER BY "font"."id" ASC)"#
        ]
        .join(" ")
    );
}

// [spec:pgorm:def:sql.ast.func/test]
#[test]
fn sub_query_with_fn() {
    #[derive(Iden)]
    #[iden = "ARRAY"]
    pub struct ArrayFunc;

    let sub_select = Query::select()
        .column(Asterisk)
        .from(Char::Table)
        .to_owned();

    let select = Query::select()
        .expr(Func::cust(ArrayFunc).arg(SimpleExpr::SubQuery(
            None,
            Box::new(sub_select.into_sub_query_statement()),
        )))
        .to_owned();

    assert_eq!(
        select.to_string(QueryBuilder),
        r#"SELECT ARRAY((SELECT * FROM "character"))"#
    );
}

#[test]
fn select_array_contains_bin_oper() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .and_where(Expr::col(Char::Character).binary(BinOper::Contains, Expr::val("test")))
            .build(QueryBuilder),
        (
            r#"SELECT "character" FROM "character" WHERE "character" @> $1"#.to_owned(),
            Values(vec!["test".into()])
        )
    );
}

#[test]
fn select_array_contained_bin_oper() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .and_where(Expr::col(Char::Character).binary(BinOper::Contained, Expr::val("test")))
            .build(QueryBuilder),
        (
            r#"SELECT "character" FROM "character" WHERE "character" <@ $1"#.to_owned(),
            Values(vec!["test".into()])
        )
    );
}

#[test]
fn select_array_overlap_bin_oper() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .and_where(Expr::col(Char::Character).binary(BinOper::Overlap, Expr::val("test")))
            .build(QueryBuilder),
        (
            r#"SELECT "character" FROM "character" WHERE "character" && $1"#.to_owned(),
            Values(vec!["test".into()])
        )
    );
}

// [spec:pgorm:req:sql.ast.expr.operators/test]
#[test]
fn get_json_field_bin_oper() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .and_where(Expr::col(Char::Character).binary(BinOper::GetJsonField, Expr::val("test")))
            .build(QueryBuilder),
        (
            r#"SELECT "character" FROM "character" WHERE "character" -> $1"#.to_owned(),
            Values(vec!["test".into()])
        )
    );
}

#[test]
fn cast_json_field_bin_oper() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .and_where(Expr::col(Char::Character).binary(BinOper::CastJsonField, Expr::val("test")))
            .build(QueryBuilder),
        (
            r#"SELECT "character" FROM "character" WHERE "character" ->> $1"#.to_owned(),
            Values(vec!["test".into()])
        )
    );
}

#[test]
fn regex_bin_oper() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .and_where(Expr::col(Char::Character).binary(BinOper::Regex, Expr::val("test")))
            .build(QueryBuilder),
        (
            r#"SELECT "character" FROM "character" WHERE "character" ~ $1"#.to_owned(),
            Values(vec!["test".into()])
        )
    );
}

#[test]
fn regex_case_insensitive_bin_oper() {
    assert_eq!(
        Query::select()
            .column(Char::Character)
            .from(Char::Table)
            .and_where(
                Expr::col(Char::Character).binary(BinOper::RegexCaseInsensitive, Expr::val("test"))
            )
            .build(QueryBuilder),
        (
            r#"SELECT "character" FROM "character" WHERE "character" ~* $1"#.to_owned(),
            Values(vec!["test".into()])
        )
    );
}

#[test]
// [spec:pgorm:req:sql.render.parens/test]
fn test_issue_674_nested_logical() {
    let t = SimpleExpr::Value(true.into());
    let f = SimpleExpr::Value(false.into());

    let x_op_y = |x, op, y| SimpleExpr::Binary(Box::new(x), op, Box::new(y));
    let t_or_t = x_op_y(t.clone(), BinOper::Or, t.clone());
    let t_or_t_or_f = x_op_y(t_or_t, BinOper::Or, f);
    let t_or_t_or_f_and_t = x_op_y(t_or_t_or_f.clone(), BinOper::And, t);

    assert_eq!(
        Query::select()
            .columns([Char::Character])
            .from(Char::Table)
            .and_where(t_or_t_or_f_and_t)
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" WHERE (TRUE OR TRUE OR FALSE) AND TRUE"#
    );
}

#[test]
// [spec:pgorm:def:sql.render.precedence/test]
fn test_issue_674_nested_comparison() {
    let int100 = SimpleExpr::Value(100i32.into());
    let int0 = SimpleExpr::Value(0i32.into());
    let int1 = SimpleExpr::Value(1i32.into());

    let x_op_y = |x, op, y| SimpleExpr::Binary(Box::new(x), op, Box::new(y));
    let t_smaller_than_t = x_op_y(int100, BinOper::SmallerThan, int0);
    let t_smaller_than_t_smaller_than_f = x_op_y(t_smaller_than_t, BinOper::SmallerThan, int1);

    assert_eq!(
        Query::select()
            .columns([Char::Character])
            .from(Char::Table)
            .and_where(t_smaller_than_t_smaller_than_f)
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" WHERE (100 < 0) < 1"#
    );
}

#[test]
fn test_issue_674_and_inside_not() {
    let t = SimpleExpr::Value(true.into());
    let f = SimpleExpr::Value(false.into());

    let op_x = |op, x| SimpleExpr::Unary(op, Box::new(x));
    let x_op_y = |x, op, y| SimpleExpr::Binary(Box::new(x), op, Box::new(y));
    let f_and_t = x_op_y(f, BinOper::And, t);
    let not_f_and_t = op_x(UnOper::Not, f_and_t);

    assert_eq!(
        Query::select()
            .columns([Char::Character])
            .from(Char::Table)
            .and_where(not_f_and_t)
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" WHERE NOT (FALSE AND TRUE)"#
    );
}

// [spec:pgorm:sem:sql.ast.condition.flattening/test]
#[test]
fn test_issue_674_nested_logical_panic() {
    let e = SimpleExpr::from(true).and(SimpleExpr::from(true).and(true.into()).and(true.into()));

    assert_eq!(
        Query::select()
            .columns([Char::Character])
            .from(Char::Table)
            .and_where(e)
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" WHERE TRUE AND (TRUE AND TRUE AND TRUE)"#
    );
}

#[test]
fn test_pgvector_select() {
    assert_eq!(
        Query::select()
            .columns([Char::Character])
            .from(Char::Table)
            .and_where(
                Expr::col(Char::Character).eq(Expr::val(pgvector::Vector::from(vec![1.0, 2.0])))
            )
            .to_string(QueryBuilder),
        r#"SELECT "character" FROM "character" WHERE "character" = '[1,2]'"#
    );
}

// [spec:pgorm:req:sql.render.cast-param-type/test]
#[test]
fn cast_param_is_pinned_to_the_source_type() {
    assert_eq!(
        Query::select()
            .expr(Expr::val(8i64).cast_as(Alias::new("BIT(8)")))
            .build(QueryBuilder),
        (
            r#"SELECT CAST($1::int8 AS BIT(8))"#.to_owned(),
            Values(vec![8i64.into()])
        )
    );

    assert_eq!(
        Query::select()
            .expr(Expr::val(vec!["a".to_owned()]).cast_as(Alias::new("tea[]")))
            .build(QueryBuilder),
        (
            r#"SELECT CAST($1::text[] AS tea[])"#.to_owned(),
            Values(vec![vec!["a".to_owned()].into()])
        )
    );

    assert_eq!(
        Query::select()
            .expr(Expr::val(json!({ "a": 1 })).cast_as(Alias::new("jsonb")))
            .build(QueryBuilder),
        (
            r#"SELECT CAST($1 AS jsonb)"#.to_owned(),
            Values(vec![json!({ "a": 1 }).into()])
        )
    );
}

// [spec:pgorm:req:sql.render.cast-param-type/test]
#[test]
fn cast_param_is_not_pinned_when_rendered_inline() {
    assert_eq!(
        Query::select()
            .expr(Expr::val(8i64).cast_as(Alias::new("BIT(8)")))
            .to_string(QueryBuilder),
        r#"SELECT CAST(8 AS BIT(8))"#
    );

    assert_eq!(
        Query::select()
            .expr(Expr::col(Char::SizeW).cast_as(Alias::new("text")))
            .build(QueryBuilder)
            .0,
        r#"SELECT CAST("size_w" AS text)"#
    );
}

// [spec:pgorm:def:sql.ast.keywords/test]    the bare-keyword expressions and their constructors
#[test]
fn keywords_1() {
    assert_eq!(
        Query::select()
            .expr(Expr::current_date())
            .expr(Expr::current_time())
            .expr(Expr::current_timestamp())
            .expr(Expr::custom_keyword(Alias::new("DEFAULT")))
            .expr(Keyword::Null)
            .to_string(QueryBuilder),
        "SELECT CURRENT_DATE, CURRENT_TIME, CURRENT_TIMESTAMP, DEFAULT, NULL"
    );
}

// [spec:pgorm:def:sql.ast.keywords/test]    `Alias` wraps an arbitrary string as an identifier,
// `NullAlias` is the empty identifier
#[test]
fn keywords_2() {
    assert_eq_unparsed!(
        Query::select()
            .expr_as(Expr::col(Glyph::Id), Alias::new("an alias"))
            .expr_as(Expr::col(Glyph::Aspect), NullAlias::new())
            .from(Glyph::Table)
            .to_string(QueryBuilder),
        r#"SELECT "id" AS "an alias", "aspect" AS "" FROM "glyph""#
    );
}

// [spec:pgorm:req:sql.ast.condition.holder+1/test]    the holder's three states: `Empty` emits no
// clause, the chain style builds `Chain`, the `cond_where` style builds `Condition`
#[test]
fn condition_holder_1() {
    // Empty.
    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph""#
    );

    // Chain: links accumulate in call order and are joined with their own operator.
    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .and_or_where(LogicalChainOper::And(Expr::col(Glyph::Aspect).eq(1)))
            .and_or_where(LogicalChainOper::Or(Expr::col(Glyph::Aspect).eq(2)))
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph" WHERE "aspect" = 1 OR "aspect" = 2"#
    );

    // Condition.
    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .cond_where(Cond::all().add(Expr::col(Glyph::Aspect).eq(1)))
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph" WHERE "aspect" = 1"#
    );
}

// [spec:pgorm:req:sql.ast.condition.holder+1/test]    a chain operator added to a holder already in
// `Condition` state panics
#[test]
#[should_panic(expected = "Cannot mix `and_where`/`or_where` and `cond_where` in statements")]
fn condition_holder_2() {
    Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .cond_where(Cond::all().add(Expr::col(Glyph::Aspect).eq(1)))
        .and_or_where(LogicalChainOper::And(Expr::col(Glyph::Aspect).eq(2)));
}

// [spec:pgorm:req:sql.ast.condition.holder+1/test]    ... and a condition added to a holder already
// in `Chain` state panics too
#[test]
#[should_panic(expected = "Cannot mix `and_where`/`or_where` and `cond_where` in statements")]
fn condition_holder_3() {
    Query::select()
        .column(Glyph::Id)
        .from(Glyph::Table)
        .and_or_where(LogicalChainOper::And(Expr::col(Glyph::Aspect).eq(1)))
        .cond_where(Cond::all().add(Expr::col(Glyph::Aspect).eq(2)));
}

// [spec:pgorm:req:sql.ast.condition.holder+1/test]    HAVING is backed by the same holder, with the
// same conjoining semantics
#[test]
fn condition_holder_3a() {
    assert_eq!(
        Query::select()
            .column(Glyph::Aspect)
            .from(Glyph::Table)
            .group_by_col(Glyph::Aspect)
            .cond_having(Cond::all().add(Expr::col(Glyph::Aspect).gt(1)))
            .and_having(Expr::col(Glyph::Aspect).lt(9))
            .cond_having(any![
                Expr::col(Glyph::Aspect).eq(3),
                Expr::col(Glyph::Aspect).eq(5)
            ])
            .to_string(QueryBuilder),
        [
            r#"SELECT "aspect" FROM "glyph" GROUP BY "aspect""#,
            r#"HAVING "aspect" > 1 AND "aspect" < 9"#,
            r#"AND ("aspect" = 3 OR "aspect" = 5)"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:sql.ast.condition.holder+1/test]    repeated `cond_where` conjoins: two
// non-negated `All` sets are appended flat, in call order
#[test]
fn condition_holder_4() {
    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .cond_where(
                Cond::all()
                    .add(Expr::col(Glyph::Aspect).eq(1))
                    .add(Expr::col(Glyph::Aspect).eq(2))
            )
            .cond_where(Cond::all().add(Expr::col(Glyph::Aspect).eq(3)))
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph" WHERE "aspect" = 1 AND "aspect" = 2 AND "aspect" = 3"#
    );
}

// [spec:pgorm:req:sql.ast.condition.holder+1/test]    anything else is combined under a fresh
// `Condition::all()`
#[test]
fn condition_holder_5() {
    // Current is a non-negated `All`, the addition is an `Any`: the addition is
    // nested inside the existing set.
    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .cond_where(Cond::all().add(Expr::col(Glyph::Aspect).eq(1)))
            .cond_where(
                Cond::any()
                    .add(Expr::col(Glyph::Aspect).eq(2))
                    .add(Expr::col(Glyph::Aspect).eq(3))
            )
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph" WHERE "aspect" = 1 AND ("aspect" = 2 OR "aspect" = 3)"#
    );

    // Current is an `Any`: both sides go under a fresh conjunction, call order kept.
    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .cond_where(
                Cond::any()
                    .add(Expr::col(Glyph::Aspect).eq(1))
                    .add(Expr::col(Glyph::Aspect).eq(2))
            )
            .cond_where(Cond::all().add(Expr::col(Glyph::Aspect).eq(3)))
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph" WHERE ("aspect" = 1 OR "aspect" = 2) AND "aspect" = 3"#
    );
}

// [spec:pgorm:req:sql.ast.with.recursive/test]    a with-clause with no CTE is refused at render
// time
#[test]
#[should_panic(expected = "Cannot build a with query that has no common table expression!")]
fn with_recursive_1() {
    WithClause::new()
        .to_owned()
        .query(Query::select().column(Glyph::Id).from(Glyph::Table).take())
        .to_string(QueryBuilder);
}

// [spec:pgorm:req:sql.ast.with.recursive/test]    a recursive with-clause holding more than one
// CTE is refused too
#[test]
#[should_panic(expected = "Cannot build a recursive query with more than one common table!")]
fn with_recursive_2() {
    let cte = |name: &str| {
        CommonTableExpression::new()
            .table_name(Alias::new(name))
            .query(Query::select().column(Glyph::Id).from(Glyph::Table).take())
            .to_owned()
    };

    WithClause::new()
        .recursive(true)
        .cte(cte("one"))
        .cte(cte("two"))
        .to_owned()
        .query(
            Query::select()
                .column(Glyph::Id)
                .from(Alias::new("one"))
                .take(),
        )
        .to_string(QueryBuilder);
}

// [spec:pgorm:req:sql.ast.with.recursive/test]    `recursive` renders `WITH RECURSIVE` and emits
// the optional SEARCH and CYCLE clauses
#[test]
fn with_recursive_3() {
    let mut base = Query::select()
        .column(Glyph::Id)
        .expr(1i32)
        .from(Glyph::Table)
        .take();
    let step = Query::select()
        .column(Glyph::Id)
        .expr(Expr::col(Alias::new("depth")).add(1i32))
        .from(Glyph::Table)
        .inner_join(
            Alias::new("cte"),
            Expr::col((Alias::new("cte"), Glyph::Id)).equals((Glyph::Table, Glyph::Id)),
        )
        .take();
    let cte = CommonTableExpression::new()
        .table_name(Alias::new("cte"))
        .column(Glyph::Id)
        .column(Alias::new("depth"))
        .query(base.union(UnionType::All, step).take())
        .to_owned();

    let search = Search::new_from_order_and_expr(
        SearchOrder::BREADTH,
        SelectExpr {
            expr: SimpleExpr::Column(ColumnRef::Column(Alias::new("depth").into_iden())),
            alias: Some(Alias::new("ordercol").into_iden()),
            window: None,
        },
    );
    let cycle = Cycle::new_from_expr_set_using(
        Expr::col(Glyph::Id),
        Alias::new("looped"),
        Alias::new("path"),
    );

    let with_clause = WithClause::new()
        .recursive(true)
        .cte(cte.clone())
        .search(search.clone())
        .cycle(cycle.clone())
        .to_owned();

    assert_eq!(
        Query::select()
            .column(Asterisk)
            .from(Alias::new("cte"))
            .take()
            .with(with_clause)
            .to_string(QueryBuilder),
        [
            r#"WITH RECURSIVE "cte" ("id", "depth") AS"#,
            r#"(SELECT "id", 1 FROM "glyph""#,
            r#"UNION ALL (SELECT "id", "depth" + 1 FROM "glyph""#,
            r#"INNER JOIN "cte" ON "cte"."id" = "glyph"."id"))"#,
            r#"SEARCH BREADTH FIRST BY "depth" SET "ordercol""#,
            r#"CYCLE "id" SET "looped" USING "path""#,
            r#"SELECT * FROM "cte""#,
        ]
        .join(" ")
    );

    // The same SEARCH / CYCLE settings are ignored when the clause is not recursive.
    let non_recursive = WithClause::new()
        .cte(cte)
        .search(search)
        .cycle(cycle)
        .to_owned();

    assert_eq!(
        Query::select()
            .column(Asterisk)
            .from(Alias::new("cte"))
            .take()
            .with(non_recursive)
            .to_string(QueryBuilder),
        [
            r#"WITH "cte" ("id", "depth") AS"#,
            r#"(SELECT "id", 1 FROM "glyph""#,
            r#"UNION ALL (SELECT "id", "depth" + 1 FROM "glyph""#,
            r#"INNER JOIN "cte" ON "cte"."id" = "glyph"."id"))"#,
            r#"SELECT * FROM "cte""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:sql.ast.with.recursive/test]    `Search` demands an aliased expression
#[test]
#[should_panic]
fn with_recursive_4() {
    Search::new_from_order_and_expr(SearchOrder::DEPTH, Expr::col(Glyph::Id));
}

// [spec:pgorm:req:sql.ast.with.recursive/test]    ... through the setter as well
#[test]
#[should_panic]
fn with_recursive_5() {
    Search::new()
        .order(SearchOrder::DEPTH)
        .expr(Expr::col(Glyph::Id));
}

// [spec:pgorm:sem:sql.render.empty-in+1/test]    an empty `IN` is rewritten to the always-false
// comparison of two distinct string values
#[test]
fn empty_in_1() {
    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .and_where(Expr::col(Glyph::Aspect).is_in(Vec::<i32>::new()))
            .build(QueryBuilder),
        (
            r#"SELECT "id" FROM "glyph" WHERE $1 = $2"#.to_owned(),
            Values(vec!["a".into(), "b".into()])
        )
    );

    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .and_where(Expr::col(Glyph::Aspect).is_in(Vec::<i32>::new()))
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph" WHERE 'a' = 'b'"#
    );
}

// [spec:pgorm:sem:sql.render.empty-in+1/test]    an empty `NOT IN` renders the always-true
// comparison instead — vacuous non-membership matches every row
#[test]
fn empty_in_2() {
    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .and_where(Expr::col(Glyph::Aspect).is_not_in(Vec::<i32>::new()))
            .build(QueryBuilder),
        (
            r#"SELECT "id" FROM "glyph" WHERE $1 = $2"#.to_owned(),
            Values(vec!["a".into(), "a".into()])
        )
    );

    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .and_where(Expr::col(Glyph::Aspect).is_not_in(Vec::<i32>::new()))
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph" WHERE 'a' = 'a'"#
    );

    // A non-empty list is unaffected.
    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .and_where(Expr::col(Glyph::Aspect).is_not_in([1, 2]))
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph" WHERE "aspect" NOT IN (1, 2)"#
    );
}

// [spec:pgorm:sem:sql.render.locking/test]    the four lock types
#[test]
fn locking_1() {
    let locked = |lock: LockType| {
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .lock(lock)
            .to_string(QueryBuilder)
    };

    assert_eq!(
        locked(LockType::Update),
        r#"SELECT "id" FROM "glyph" FOR UPDATE"#
    );
    assert_eq!(
        locked(LockType::NoKeyUpdate),
        r#"SELECT "id" FROM "glyph" FOR NO KEY UPDATE"#
    );
    assert_eq!(
        locked(LockType::Share),
        r#"SELECT "id" FROM "glyph" FOR SHARE"#
    );
    assert_eq!(
        locked(LockType::KeyShare),
        r#"SELECT "id" FROM "glyph" FOR KEY SHARE"#
    );

    // The named shorthands.
    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .lock_exclusive()
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph" FOR UPDATE"#
    );
    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .lock_shared()
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph" FOR SHARE"#
    );
}

// [spec:pgorm:sem:sql.render.locking/test]    ` OF ` names the locked tables, comma-separated
// and quoted, and the behaviour follows
#[test]
fn locking_2() {
    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .from(Char::Table)
            .lock_with_tables(
                LockType::Update,
                [Glyph::Table.into_table_ref(), Char::Table.into_table_ref()]
            )
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph", "character" FOR UPDATE OF "glyph", "character""#
    );

    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .lock_with_behavior(LockType::Update, LockBehavior::Nowait)
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph" FOR UPDATE NOWAIT"#
    );

    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .lock_with_tables_behavior(LockType::Share, [Glyph::Table], LockBehavior::SkipLocked)
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph" FOR SHARE OF "glyph" SKIP LOCKED"#
    );

    // The clause is overwritten wholesale — the last call wins.
    assert_eq!(
        Query::select()
            .column(Glyph::Id)
            .from(Glyph::Table)
            .lock_with_behavior(LockType::Update, LockBehavior::Nowait)
            .lock_shared()
            .to_string(QueryBuilder),
        r#"SELECT "id" FROM "glyph" FOR SHARE"#
    );
}
