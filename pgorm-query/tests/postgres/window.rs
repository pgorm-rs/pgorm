use super::*;
use pretty_assertions::assert_eq;

// [spec:pgorm:def:sql.ast.window-statement/test]    PARTITION BY accumulates from all four entry
// points
// [spec:pgorm:req:sql.render.window/test]    an inline window renders ` OVER ( … )`
#[test]
fn window_1() {
    assert_eq!(
        Query::select()
            .from(Char::Table)
            .expr_window(
                Expr::col(Char::Character),
                WindowStatement::partition_by(Char::FontSize)
            )
            .to_string(QueryBuilder),
        r#"SELECT "character" OVER ( PARTITION BY "font_size" ) FROM "character""#
    );

    assert_eq!(
        Query::select()
            .from(Char::Table)
            .expr_window_as(
                Expr::col(Char::Character),
                WindowStatement::partition_by_custom("\"font_size\"")
                    .partition_by(Char::SizeW)
                    .partition_by_columns([Char::SizeH])
                    .partition_by_customs(["\"font_id\""])
                    .take(),
                Alias::new("C")
            )
            .to_string(QueryBuilder),
        [
            r#"SELECT "character" OVER ("#,
            r#"PARTITION BY "font_size", "size_w", "size_h", "font_id" ) AS "C""#,
            r#"FROM "character""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:def:sql.ast.window-statement/test]    ORDER BY comes from the shared
// `OrderedStatement` trait
// [spec:pgorm:req:sql.render.window/test]    ` PARTITION BY … ORDER BY …`
#[test]
fn window_2() {
    assert_eq!(
        Query::select()
            .from(Char::Table)
            .expr_window_as(
                Expr::col(Char::Character),
                WindowStatement::partition_by(Char::FontSize)
                    .order_by(Char::Id, Order::Asc)
                    .order_by(Char::SizeW, Order::Desc)
                    .take(),
                Alias::new("C")
            )
            .to_string(QueryBuilder),
        [
            r#"SELECT "character" OVER ("#,
            r#"PARTITION BY "font_size" ORDER BY "id" ASC, "size_w" DESC ) AS "C""#,
            r#"FROM "character""#,
        ]
        .join(" ")
    );

    // A window with no partition at all is just its ORDER BY.
    assert_eq!(
        Query::select()
            .from(Char::Table)
            .expr_window(
                Expr::col(Char::Character),
                WindowStatement::new().order_by(Char::Id, Order::Asc).take()
            )
            .to_string(QueryBuilder),
        r#"SELECT "character" OVER (  ORDER BY "id" ASC ) FROM "character""#
    );
}

// [spec:pgorm:def:sql.ast.window-statement/test]    `frame_start` sets a single bound,
// `frame_between` sets both, for either frame type
// [spec:pgorm:req:sql.render.window/test]    ` RANGE `/` ROWS ` then `BETWEEN start AND end` or
// the start bound alone
#[test]
fn window_3() {
    assert_eq!(
        Query::select()
            .from(Char::Table)
            .expr_window(
                Expr::col(Char::Character),
                WindowStatement::partition_by(Char::FontSize)
                    .frame_start(FrameType::Rows, Frame::UnboundedPreceding)
                    .take()
            )
            .to_string(QueryBuilder),
        [
            r#"SELECT "character" OVER ("#,
            r#"PARTITION BY "font_size" ROWS UNBOUNDED PRECEDING )"#,
            r#"FROM "character""#,
        ]
        .join(" ")
    );

    assert_eq!(
        Query::select()
            .from(Char::Table)
            .expr_window(
                Expr::col(Char::Character),
                WindowStatement::partition_by(Char::FontSize)
                    .frame_between(
                        FrameType::Range,
                        Frame::UnboundedPreceding,
                        Frame::UnboundedFollowing
                    )
                    .take()
            )
            .to_string(QueryBuilder),
        [
            r#"SELECT "character" OVER ("#,
            r#"PARTITION BY "font_size" RANGE BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING )"#,
            r#"FROM "character""#,
        ]
        .join(" ")
    );

    // The last `frame`/`frame_start`/`frame_between` call wins.
    assert_eq!(
        Query::select()
            .from(Char::Table)
            .expr_window(
                Expr::col(Char::Character),
                WindowStatement::partition_by(Char::FontSize)
                    .frame_between(
                        FrameType::Range,
                        Frame::UnboundedPreceding,
                        Frame::UnboundedFollowing
                    )
                    .frame_start(FrameType::Rows, Frame::CurrentRow)
                    .take()
            )
            .to_string(QueryBuilder),
        [
            r#"SELECT "character" OVER ("#,
            r#"PARTITION BY "font_size" ROWS CURRENT ROW )"#,
            r#"FROM "character""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:sql.render.window/test]    a bounded offset renders as a parameter immediately
// followed by the keyword, with no separating space
#[test]
fn window_4() {
    assert_eq!(
        Query::select()
            .from(Char::Table)
            .expr_window(
                Expr::col(Char::Character),
                WindowStatement::partition_by(Char::FontSize)
                    .frame_between(FrameType::Rows, Frame::Preceding(2), Frame::Following(3))
                    .take()
            )
            .build(QueryBuilder),
        (
            [
                r#"SELECT "character" OVER ("#,
                r#"PARTITION BY "font_size" ROWS BETWEEN $1PRECEDING AND $2FOLLOWING )"#,
                r#"FROM "character""#,
            ]
            .join(" "),
            Values(vec![Value::Unsigned(Some(2)), Value::Unsigned(Some(3))])
        )
    );

    // Inline, the same shape drops the placeholder but keeps the missing space.
    assert_eq!(
        Query::select()
            .from(Char::Table)
            .expr_window(
                Expr::col(Char::Character),
                WindowStatement::partition_by(Char::FontSize)
                    .frame_start(FrameType::Rows, Frame::Preceding(2))
                    .take()
            )
            .to_string(QueryBuilder),
        [
            r#"SELECT "character" OVER ("#,
            r#"PARTITION BY "font_size" ROWS 2PRECEDING )"#,
            r#"FROM "character""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:def:sql.ast.window-statement/test]    `WindowSelectType::Name` references a window
// declared at statement level with `SelectStatement::window`
// [spec:pgorm:req:sql.render.window/test]    a named reference renders ` OVER "name"`
#[test]
fn window_5() {
    assert_eq!(
        Query::select()
            .from(Char::Table)
            .expr_window_name(Expr::col(Char::Character), Alias::new("w"))
            .window(
                Alias::new("w"),
                WindowStatement::partition_by(Char::FontSize)
            )
            .to_string(QueryBuilder),
        [
            r#"SELECT "character" OVER "w" FROM "character""#,
            r#"WINDOW "w" AS PARTITION BY "font_size""#,
        ]
        .join(" ")
    );

    assert_eq!(
        Query::select()
            .from(Char::Table)
            .expr_window_name_as(Expr::col(Char::Character), Alias::new("w"), Alias::new("C"))
            .window(
                Alias::new("w"),
                WindowStatement::partition_by(Char::FontSize)
            )
            .to_string(QueryBuilder),
        [
            r#"SELECT "character" OVER "w" AS "C" FROM "character""#,
            r#"WINDOW "w" AS PARTITION BY "font_size""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:def:sql.ast.window-statement/test]    the statement holds at most one named window:
// a second `window()` call replaces the first
#[test]
fn window_6() {
    assert_eq!(
        Query::select()
            .from(Char::Table)
            .expr_window_name(Expr::col(Char::Character), Alias::new("w2"))
            .window(
                Alias::new("w1"),
                WindowStatement::partition_by(Char::FontSize)
            )
            .window(Alias::new("w2"), WindowStatement::partition_by(Char::SizeW))
            .to_string(QueryBuilder),
        [
            r#"SELECT "character" OVER "w2" FROM "character""#,
            r#"WINDOW "w2" AS PARTITION BY "size_w""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:def:sql.ast.window-statement/test]    `take()` moves the contents out and leaves the
// builder empty
#[test]
fn window_7() {
    let mut window = WindowStatement::partition_by(Char::FontSize);
    window.order_by(Char::Id, Order::Asc);
    window.frame_start(FrameType::Rows, Frame::CurrentRow);

    let taken = window.take();
    assert_eq!(window, WindowStatement::new());
    assert_ne!(taken, WindowStatement::new());
}
