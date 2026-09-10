"""Interpret scalar operations independently of pgorm's expression builders."""

from .comparison import InvalidOracle
from .reference_literal import literal
from .reference_sql import (
    SQL,
    Table,
    binary,
    bound,
    condition,
    expression,
    join,
)
from .reference_values import qualified, quote


def pattern(value, text, method, escape):
    if text["sql_null"] or text["type"]["kind"] != "text":
        raise InvalidOracle("pattern oracle requires a non-null text argument")
    argument = text["data"]
    if method in ("contains_text", "starts_with", "ends_with"):
        # Escaping is the reference's own rule, not the subject's helper output.
        argument = (
            argument.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")
        )
        argument = (
            ("%" if method != "starts_with" else "")
            + argument
            + ("%" if method != "ends_with" else "")
        )
        escape, method = "\\", "like"
    keyword = {
        "like": "LIKE",
        "not_like": "NOT LIKE",
        "ilike": "ILIKE",
        "not_ilike": "NOT ILIKE",
    }[method]
    result = "(" + value + " " + keyword + " " + bound({**text, "data": argument})
    if escape is not None:
        result += " ESCAPE " + bound({**text, "data": escape})
    return result + ")"


def scalar_node(name, i, d):
    match name:
        case "value":
            return d["value"]
        case "name":
            value = i["value"]
            if value["sql_null"] or value["type"]["kind"] != "text":
                raise InvalidOracle("identifier oracle requires a non-null text value")
            quote(value["data"])
            return value["data"]
        case "table":
            return Table(i.get("name", d.get("name")), d.get("schema"), d.get("alias"))
        case "expr.column":
            name = i.get("name", d.get("name"))
            return i["table"].column(name) if "table" in i else SQL((quote(name),))
        case "expr.value":
            return literal(i["value"]) if d["mode"] == "literal" else bound(i["value"])
        case "expr.binary":
            return binary(i["left"], i["right"], d["operator"])
        case "expr.unary":
            value = i["value"]
            if d["operator"] == "not":
                return "(NOT " + value + ")"
            return (
                "("
                + value
                + (" IS NULL)" if d["operator"] == "is_null" else " IS NOT NULL)")
            )
        case "expr.membership":
            if not i["items"]:
                return SQL(("TRUE" if d["negated"] else "FALSE",))
            return (
                "("
                + i["value"]
                + (" NOT IN (" if d["negated"] else " IN (")
                + join([bound(item) for item in i["items"]])
                + "))"
            )
        case "expr.pattern":
            return pattern(i["value"], i["pattern"], d["method"], d.get("escape"))
        case "expr.call":
            return (
                quote(d["name"])
                + "("
                + join([expression(arg) for arg in i["arguments"]])
                + ")"
            )
        case "expr.cast":
            return (
                "("
                + i["value"]
                + ")::"
                + qualified(d["name"], d.get("schema"))
                + ("[]" if d.get("array") else "")
            )
        case "expr.alias":
            return i["value"] + " AS " + quote(i.get("name", d.get("name")))
        case "expr.order":
            result = i["value"] + " " + d["direction"].upper()
            return result + (
                " NULLS " + d["nulls"].upper() if d["nulls"] != "default" else ""
            )
        case "condition":
            return condition(i["items"], d["mode"], d.get("negated", False))
    raise InvalidOracle("independent expression semantics uncovered: " + name)
