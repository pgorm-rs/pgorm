use serde_json::{Map, Value, json};

pub(crate) fn operations() -> Map<String, Value> {
    [
        ("identifier", "pgorm_query::Alias"),
        ("col", "pgorm_query::Expr::col"),
        ("bind", "pgorm_query::Expr::value"),
        ("literal", "pgorm_query::SimpleExpr::Constant"),
        ("tuple_expr", "pgorm_query::Expr::tuple"),
        ("call", "pgorm_query::Func"),
        ("expr.eq", "pgorm_query::SimpleExpr::binary(BinOper::Equal)"),
        (
            "expr.ne",
            "pgorm_query::SimpleExpr::binary(BinOper::NotEqual)",
        ),
        (
            "expr.gt",
            "pgorm_query::SimpleExpr::binary(BinOper::GreaterThan)",
        ),
        (
            "expr.gte",
            "pgorm_query::SimpleExpr::binary(BinOper::GreaterThanOrEqual)",
        ),
        (
            "expr.lt",
            "pgorm_query::SimpleExpr::binary(BinOper::SmallerThan)",
        ),
        (
            "expr.lte",
            "pgorm_query::SimpleExpr::binary(BinOper::SmallerThanOrEqual)",
        ),
        ("expr.add", "pgorm_query::SimpleExpr::binary(BinOper::Add)"),
        ("expr.sub", "pgorm_query::SimpleExpr::binary(BinOper::Sub)"),
        ("expr.mul", "pgorm_query::SimpleExpr::binary(BinOper::Mul)"),
        ("expr.div", "pgorm_query::SimpleExpr::binary(BinOper::Div)"),
        ("expr.mod", "pgorm_query::SimpleExpr::binary(BinOper::Mod)"),
        ("expr.and", "pgorm_query::SimpleExpr::and"),
        ("expr.or", "pgorm_query::SimpleExpr::or"),
        ("expr.not", "pgorm_query::SimpleExpr::not"),
        ("expr.is_null", "pgorm_query::Expr::is_null"),
        ("expr.is_not_null", "pgorm_query::Expr::is_not_null"),
        ("expr.is_in", "pgorm_query::Expr::is_in"),
        ("expr.is_not_in", "pgorm_query::Expr::is_not_in"),
        ("expr.between", "pgorm_query::Expr::between"),
        ("expr.not_between", "pgorm_query::Expr::not_between"),
        ("expr.cast", "pgorm_query::SimpleExpr::cast_as_type"),
        ("expr.starts_with", "pgorm_query::Func::starts_with"),
        (
            "expr.contains_text",
            "pgorm_query::Func::cust(strpos), Expr::gt",
        ),
        (
            "expr.ends_with",
            "pgorm_query::Func::cust(right), Func::char_length, Expr::eq",
        ),
        ("like_pattern", "pgorm_query::LikeExpr"),
        ("expr.like", "pgorm_query::SimpleExpr::like"),
        ("expr.not_like", "pgorm_query::SimpleExpr::not_like"),
        ("expr.ilike", "pgorm_query::Expr::ilike"),
        ("expr.not_ilike", "pgorm_query::Expr::not_ilike"),
        (
            "expr.inspect",
            "pgorm_query::SelectStatement::expr, SelectStatement::build",
        ),
        ("condition.all", "pgorm_query::Condition::all"),
        ("condition.any", "pgorm_query::Condition::any"),
        ("condition.add", "pgorm_query::Condition::add"),
        ("condition.not", "pgorm_query::Condition::not"),
        (
            "condition.inspect",
            "pgorm_query::SelectStatement::cond_where, SelectStatement::build",
        ),
        ("order_by", "pgorm_query::Order, pgorm_query::NullOrdering"),
        ("expr.as_", "pgorm_query::SimpleExpr, pgorm_query::Alias"),
    ]
    .into_iter()
    .map(|(name, rust_api)| (name.into(), json!({"rust_api": rust_api, "features": []})))
    .collect()
}
