"""Recursive scalar and predicate productions over available typed sources."""

from .grammar_state import Expression


def scalar(state, sources, kind, depth):
    choices = [
        (source, field)
        for source in sources
        for field in source.fields
        if field.kind == kind
    ]
    production = state.choices.take(
        ("column", "value", "arithmetic")
        if kind == "i32" and depth
        else ("column", "value")
    )
    if production == "column" and choices:
        source, field = state.choices.take(choices)
        return state.column(source, field.name)
    if production == "arithmetic":
        left = scalar(state, sources, kind, depth - 1)
        right = state.constant("i32", state.choices.integer(0, 8))
        return state.binary(left, right, state.choices.take(("add", "sub")))
    return state.constant(
        kind, state.text() if kind == "text" else state.choices.integer(-3, 40)
    )


# [spec:pgorm:req:generative.grammar]
def predicate(state, sources, depth):
    production = state.choices.take(
        ("compare", "null", "membership", "pattern", "all", "any", "not")
        if depth
        else ("compare", "null", "membership", "pattern")
    )
    if production in ("all", "any"):
        items = [
            predicate(state, sources, depth - 1).node
            for _ in range(state.choices.integer(0, 2))
        ]
        node = state.node(
            "condition",
            {"items": items},
            {"mode": production, "negated": state.choices.take((False, True))},
        )
        return Expression(node, "bool", True)
    if production == "not":
        value = predicate(state, sources, depth - 1)
        if (
            next(node for node in state.author.nodes if node["id"] == value.node)["op"]
            == "condition"
        ):
            node = state.node(
                "condition", {"items": [value.node]}, {"mode": "all", "negated": True}
            )
        else:
            node = state.node("expr.unary", {"value": value.node}, {"operator": "not"})
        return Expression(node, "bool", value.nullable)
    if production == "pattern":
        value = scalar(state, sources, "text", 0)
        method = state.choices.take(
            (
                "contains_text",
                "starts_with",
                "ends_with",
                "like",
                "not_like",
                "ilike",
                "not_ilike",
            )
        )
        pattern = (
            state.text()
            if method in ("contains_text", "starts_with", "ends_with")
            else state.choices.take(("%", "_%", "a%b_c", r"\%"))
        )
        node = state.node(
            "expr.pattern",
            {"value": value.node, "pattern": state.value("text", pattern)},
            {"method": method},
        )
        return Expression(node, "bool", value.nullable)
    value = scalar(state, sources, "i32", min(depth, 1))
    if production == "null":
        node = state.node(
            "expr.unary",
            {"value": value.node},
            {"operator": state.choices.take(("is_null", "is_not_null"))},
        )
        return Expression(node, "bool", False)
    if production == "membership":
        items = [
            state.value("i32", state.choices.integer(0, 4))
            for _ in range(state.choices.integer(0, 3))
        ]
        if state.choices.take((False, True)):
            items.append(state.value("i32", None, sql_null=True))
        node = state.node(
            "expr.membership",
            {"value": value.node, "items": items},
            {"negated": state.choices.take((False, True))},
        )
        return Expression(node, "bool", True)
    right = scalar(state, sources, "i32", min(depth, 1))
    return state.binary(
        value, right, state.choices.take(("eq", "ne", "lt", "lte", "gt", "gte"))
    )
