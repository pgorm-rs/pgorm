"""Generate projections, joins and modifiers around recursive expressions."""

from .grammar_expr import predicate, scalar


def _joins(state, base):
    sources, joins = [base], []
    for _ in range(state.choices.integer(0, 2)):
        kind = state.choices.take(("inner", "left", "cross"))
        name = state.choices.take(("notes", "accounts"))
        other = state.source(name, optional=kind == "left")
        left = state.column(base, "id")
        right = state.column(other, "account_id" if name == "notes" else "id")
        on = state.binary(left, right, state.choices.take(("eq", "lte")))
        sources.append(other)
        joins.append((other, kind, on))
    return sources, joins


def _order(state, query, columns):
    keys = []
    for column in columns:
        node = next(node for node in state.author.nodes if node["id"] == column)
        value = state.node("expr.column", data={"name": node["data"]["name"]})
        keys.append(
            state.node(
                "expr.order",
                {"value": value},
                {
                    "direction": state.choices.take(("asc", "desc")),
                    "nulls": state.choices.take(("first", "last", "default")),
                },
            )
        )
    return state.node("select.order", {"query": query, "keys": keys})


# [spec:pgorm:req:generative.grammar]
def select(state):
    base = state.source()
    sources, joins = _joins(state, base)
    nonce = state.constant("i64", state.index)
    columns = [state.node("expr.alias", {"value": nonce.node}, {"name": "nonce"})]
    for index in range(state.choices.integer(1, 3)):
        expression = scalar(
            state, sources, state.choices.take(("i32", "text")), state.limits.depth
        )
        columns.append(
            state.node(
                "expr.alias",
                {"value": expression.node},
                {"name": state.name("p" + str(index))},
            )
        )
    query = state.node("select", {"columns": columns})
    query = state.node("select.from", {"query": query, "table": base.node})
    for source, kind, on in joins:
        inputs = {"query": query, "table": source.node}
        if kind != "cross":
            inputs["on"] = on.node
        query = state.node("select.join", inputs, {"kind": kind})
    for _ in range(state.choices.integer(1, 2)):
        expression = predicate(state, sources, state.limits.depth)
        query = state.node(
            "select.filter", {"query": query, "predicate": expression.node}
        )
    if state.choices.take((False, True)):
        query = state.node("select.distinct", {"query": query})
    # Ordering by every projected expression makes LIMIT deterministic even for joins.
    ordered = state.choices.take((False, True))
    if ordered:
        query = _order(state, query, columns)
        query = state.node(
            "select.page",
            {"query": query},
            {
                "limit": state.choices.integer(0, 8),
                "offset": state.choices.integer(0, 2),
            },
        )
    if state.choices.take((False, True)):
        if not ordered:
            # A stopped stream observes a prefix, so its order must be defined.
            query = _order(state, query, columns)
        state.author.effect(
            "stream",
            {"query": query},
            {
                "take": state.choices.integer(0, 8),
                "cancel": state.choices.take((False, True)),
                "ordered": True,
            },
        )
    else:
        scope = "root"
        if state.choices.take((False, True)):
            state.author.effect(
                "begin",
                data={
                    "child": "tx",
                    "mode": "read_only",
                    "isolation": state.choices.take(
                        ("read_committed", "repeatable_read")
                    ),
                },
            )
            scope = "tx"
        state.fetch(query, scope=scope, ordered=ordered)
        if scope == "tx":
            state.author.effect(state.choices.take(("commit", "rollback")), scope=scope)
