"""CRUD sequences with proven result availability and nested transaction scopes."""

from . import baseline, wire


def account_row(state, identity):
    definition = baseline.default()["tables"][0]
    values = dict(
        zip(
            (column["name"] for column in definition["columns"]),
            definition["rows"][0],
            strict=True,
        )
    )
    values.update(
        id=identity,
        name=state.text(),
        rank=state.choices.integer(0, 3),
        score=state.choices.take((None, -10, 0, 30)),
    )
    nodes = []
    for column in definition["columns"]:
        kind, value = column["kind"], values[column["name"]]
        if isinstance(kind, dict):
            kind = {"kind": "enum", "schema": kind["enum"][0], "name": kind["enum"][1]}
        elif kind == "text[]":
            kind = {"kind": "array", "element": {"kind": "text"}}
            value = [wire.scalar("text", item, sql_null=item is None) for item in value]
        elif kind == "timestamp":
            kind = "datetime"
        elif kind == "timestamptz":
            kind, value = "datetime_utc", value.removesuffix("+00") + "+00:00"
        nodes.append(state.value(kind, value, sql_null=value is None))
    return list(values), nodes


def _table(state):
    return state.node("table", data={"schema": "fixture", "name": "accounts"})


def _column(state, table, name):
    return state.node("expr.column", {"table": table}, {"name": name})


def _guard(state, table, identity):
    tenant = state.node(
        "expr.binary",
        {"left": _column(state, table, "tenant"), "right": state.value("i32", 1)},
        {"operator": "eq"},
    )
    row = state.node(
        "expr.binary",
        {"left": _column(state, table, "id"), "right": identity},
        {"operator": "eq"},
    )
    return state.node("condition", {"items": [tenant, row]}, {"mode": "all"})


def _insert(state, table, scope):
    columns, values = account_row(state, 100 + state.index)
    query = state.node(
        "insert",
        {"table": table},
        {"columns": columns},
    )
    query = state.node(
        "insert.row",
        {"query": query, "values": values},
    )
    conflict = state.choices.take((False, True))
    if conflict:
        action = state.choices.take(("nothing", "update"))
        query = state.node(
            "insert.conflict",
            {"query": query},
            {
                "keys": ["id"],
                "action": action,
                "columns": ["name"] if action == "update" else [],
            },
        )
    returning = [_column(state, table, name) for name in ("id", "name")]
    query = state.node("write.returning", {"query": query, "columns": returning})
    step = state.fetch(query, scope=scope)
    if conflict:
        # The first insert proves row availability for the result references;
        # the second actually reaches the selected conflict action.
        state.fetch(query, scope=scope)
    result = {
        name: state.node(
            "result.value",
            data={"step": step, "row": 0, "column": name, "type": {"kind": kind}},
        )
        for name, kind in (("id", "i32"), ("name", "text"))
    }
    return result


def _update(state, table, result, scope):
    query = state.node("update", {"table": table})
    query = state.node(
        "update.set", {"query": query, "value": result["name"]}, {"column": "name"}
    )
    guard = _guard(state, table, state.value("i32", state.choices.take((1, 2, 4))))
    query = state.node("write.filter", {"query": query, "predicate": guard})
    if state.choices.take((False, True)):
        query = state.node(
            "write.returning",
            {"query": query, "columns": [_column(state, table, "id")]},
        )
        state.fetch(query, scope=scope)
    else:
        state.author.effect("execute", {"query": query}, scope=scope)


def _read(state, table, result, scope):
    query = state.node(
        "select",
        {"columns": [_column(state, table, name) for name in ("id", "name", "score")]},
    )
    query = state.node("select.from", {"query": query, "table": table})
    query = state.node(
        "select.filter",
        {"query": query, "predicate": _guard(state, table, result["id"])},
    )
    state.fetch(query, scope=scope)


# [spec:pgorm:req:generative.grammar]
def sequence(state):
    table, scope = _table(state), "root"
    transaction = state.choices.take((False, True))
    if transaction:
        state.author.effect(
            "begin",
            data={
                "child": "tx",
                "mode": "read_write",
                "isolation": state.choices.take(
                    ("read_committed", "repeatable_read", "serializable")
                ),
            },
        )
        scope = "tx"
    result = _insert(state, table, scope)
    for _ in range(state.choices.integer(1, state.limits.stages)):
        _update(state, table, result, scope) if state.choices.take(
            (False, True)
        ) else _read(state, table, result, scope)
    if transaction and state.choices.take((False, True)):
        state.author.effect(
            "begin",
            data={"child": "nested", "mode": "default", "isolation": "default"},
            scope=scope,
        )
        _update(state, table, result, "nested")
        state.author.effect(state.choices.take(("commit", "rollback")), scope="nested")
    if state.choices.take((False, True)):
        query = state.node("delete", {"table": table})
        query = state.node(
            "write.filter",
            {"query": query, "predicate": _guard(state, table, result["id"])},
        )
        state.author.effect("execute", {"query": query}, scope=scope)
    if transaction:
        state.author.effect(state.choices.take(("commit", "rollback")), scope="tx")


def rejection(state):
    """An intentional database rejection; never relabel a valid-mode failure."""
    rule = state.choices.take(("division", "not-null", "duplicate"))
    if rule == "division":
        left = state.constant("i32", state.choices.integer(1, 100))
        right = state.constant("i32", 0)
        expression = state.binary(left, right, "div")
        query = state.node(
            "select",
            {"columns": [expression.node, state.constant("i64", state.index).node]},
        )
        state.fetch(query, error={"class": "DatabaseError", "cause": "sqlstate:22012"})
    elif rule == "not-null":
        table = _table(state)
        query = state.node("update", {"table": table})
        query = state.node(
            "update.set",
            {"query": query, "value": state.value("text", None, sql_null=True)},
            {"column": "name"},
        )
        query = state.node(
            "write.filter",
            {"query": query, "predicate": _guard(state, table, state.value("i32", 1))},
        )
        state.author.effect(
            "execute",
            {"query": query},
            error={"class": "DatabaseError", "cause": "sqlstate:23502"},
        )
    else:
        table = _table(state)
        columns, values = account_row(state, 1)
        query = state.node("insert", {"table": table}, {"columns": columns})
        query = state.node("insert.row", {"query": query, "values": values})
        state.author.effect(
            "execute",
            {"query": query},
            error={"class": "DatabaseError", "cause": "sqlstate:23505"},
        )
    return rule
