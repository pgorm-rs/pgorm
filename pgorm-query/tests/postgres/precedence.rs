use super::*;
use crate::oracle::{assert_eq, parsed_nodes};

/// An expression as the grammar grouped it: a boolean node as its operator
/// over its arguments, an operator expression as its operator name — over its
/// operands, a BETWEEN's two bounds among them, when any of those is not an
/// atom — and anything else as `_`.
fn grouping(node: &serde_json::Value) -> String {
    if let Some(boolean) = node.get("BoolExpr") {
        let operator = match boolean["boolop"].as_i64() {
            Some(1) => "AND",
            Some(2) => "OR",
            Some(3) => "NOT",
            other => panic!("no boolean operator {other:?}"),
        };
        format!("{operator}({})", groupings(&boolean["args"]).join(", "))
    } else if let Some(operation) = node.get("AExpr") {
        let name = operation["name"][0]["String"]["sval"]
            .as_str()
            .unwrap_or_default();
        let operands: Vec<String> = [&operation["lexpr"], &operation["rexpr"]]
            .into_iter()
            .flat_map(|operand| match operand.get("List") {
                Some(list) => groupings(&list["items"]),
                None => vec![grouping(operand)],
            })
            .collect();
        if operands.iter().all(|operand| operand == "_") {
            name.to_owned()
        } else {
            format!("{name}({})", operands.join(", "))
        }
    } else {
        "_".to_owned()
    }
}

fn groupings(nodes: &serde_json::Value) -> Vec<String> {
    nodes
        .as_array()
        .into_iter()
        .flatten()
        .map(grouping)
        .collect()
}

fn where_grouping(sql: &str) -> String {
    let select = parsed_nodes(sql, "SelectStmt")
        .into_iter()
        .next()
        .expect("a SELECT");
    grouping(&select["where_clause"])
}

// [spec:pgorm:def:sql.render.precedence+11/test]    every boolean-valued operator outside the
// comparison, BETWEEN, IN, LIKE and IS families renders bare under AND, OR and NOT, and the grammar
// groups the bare render as the condition was built
#[test]
fn boolean_operators_render_bare_under_logic() {
    let operators = [
        (BinOper::Contains, "@>"),
        (BinOper::Contained, "<@"),
        (BinOper::Overlap, "&&"),
        (BinOper::Matches, "@@"),
        (BinOper::Similarity, "%"),
        (BinOper::WordSimilarity, "<%"),
        (BinOper::StrictWordSimilarity, "<<%"),
        (BinOper::HasJsonKey, "?"),
        (BinOper::HasAnyJsonKeys, "?|"),
        (BinOper::HasAllJsonKeys, "?&"),
        (BinOper::Regex, "~"),
        (BinOper::RegexCaseInsensitive, "~*"),
    ];
    for (operator, lexeme) in operators {
        let test = |column: Char, value: i32| Expr::col(column).binary(operator, value);
        let sql = Query::select()
            .column(Char::Id)
            .from(Char::Table)
            .cond_where(
                Condition::all()
                    .add(test(Char::Character, 1))
                    .add(
                        Condition::any()
                            .add(test(Char::SizeW, 2))
                            .add(test(Char::SizeH, 3)),
                    )
                    .add(test(Char::FontId, 4).not()),
            )
            .to_string();
        assert_eq!(
            sql,
            format!(
                r#"SELECT "id" FROM "character" WHERE "character" {lexeme} 1 AND ("size_w" {lexeme} 2 OR "size_h" {lexeme} 3) AND (NOT "font_id" {lexeme} 4)"#
            )
        );
        assert_eq!(
            where_grouping(&sql),
            format!("AND({lexeme}, OR({lexeme}, {lexeme}), NOT({lexeme}))")
        );
    }
}
