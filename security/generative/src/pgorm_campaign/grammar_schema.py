"""Fixture-owned schema transitions and stored values reused as names."""

from . import baseline


def _read(state, table, columns):
    projections = [
        state.node("expr.column", {"table": table}, {"name": name}) for name in columns
    ]
    query = state.node("select", {"columns": projections})
    query = state.node("select.from", {"query": query, "table": table})
    return state.fetch(query)


def _enum(state, schema):
    name = state.name("enum" + str(state.index))
    first = state.name("first")
    second = state.name("second")
    query = state.node(
        "schema.enum", data={"name": name, "schema": schema, "labels": [first]}
    )
    state.author.effect("execute", {"query": query})
    for method, arguments in (
        ("add", {"value": second}),
        ("rename", {"value": second, "new_value": state.name("renamed")}),
        ("drop", {}),
    ):
        query = state.node(
            "schema.enum_change",
            data={"name": name, "schema": schema, "method": method, **arguments},
        )
        state.author.effect("execute", {"query": query})


def _insert(state, table, columns):
    query = state.node("insert", {"table": table}, {"columns": columns})
    for index in range(state.choices.integer(1, 3)):
        query = state.node(
            "insert.row",
            {
                "query": query,
                "values": [
                    state.value("i32", state.index + index),
                    state.value("text", state.text()),
                ],
            },
        )
    state.author.effect("execute", {"query": query})
    if state.choices.take((False, True)):
        query = state.node("insert", {"table": table}, {"columns": []})
        query = state.node("insert.defaults", {"query": query})
        state.author.effect("execute", {"query": query})


def _stored_identifier(state, table, columns):
    stored_name = state.name("stored")
    query = state.node("update", {"table": table})
    query = state.node(
        "update.set",
        {"query": query, "value": state.value("text", stored_name)},
        {"column": columns[1]},
    )
    query = state.node("write.all", {"query": query})
    state.author.effect("execute", {"query": query})
    step = _read(state, table, columns)
    value = state.node(
        "result.value",
        data={"step": step, "row": 0, "column": columns[1], "type": {"kind": "text"}},
    )
    name = state.node("name", {"value": value})
    expression = state.constant("i64", state.index)
    projection = state.node("expr.alias", {"value": expression.node, "name": name})
    state.fetch(state.node("select", {"columns": [projection]}))


# [spec:pgorm:req:generative.grammar]
def schema(state):
    schema = state.choices.take(("fixture", "other", state.name("campaign_")))
    if schema not in ("fixture", "other"):
        state.author.fixture["tables"].append(
            {
                "schema": schema,
                "name": "anchor",
                "columns": [baseline.column("id", "i32")],
                "rows": [[1]],
            }
        )
    table_name = state.name("table" + str(state.index))
    columns = [state.name("number"), state.name("text")]
    table = state.node("table", data={"schema": schema, "name": table_name})
    declarations = [
        baseline.column(columns[0], "i32", nullable=True),
        baseline.column(columns[1], "text", nullable=True),
    ]
    query = state.node("schema.create", {"table": table}, {"columns": declarations})
    state.author.effect("execute", {"query": query})
    _insert(state, table, columns)
    _stored_identifier(state, table, columns)
    if state.choices.take((False, True)):
        query = state.node(
            "schema.index",
            {"table": table},
            {"name": state.name("index"), "columns": columns[:1], "unique": False},
        )
        state.author.effect("execute", {"query": query})
    if state.choices.take((False, True)):
        renamed = state.name("column")
        query = state.node(
            "schema.rename", {"table": table}, {"name": renamed, "column": columns[1]}
        )
        state.author.effect("execute", {"query": query})
        columns[1] = renamed
    if state.choices.take((False, True)):
        renamed = state.name("renamed")
        query = state.node("schema.rename", {"table": table}, {"name": renamed})
        state.author.effect("execute", {"query": query})
        table = state.node("table", data={"schema": schema, "name": renamed})
    _read(state, table, columns)
    query = state.node("schema.drop", {"table": table})
    state.author.effect("execute", {"query": query})
    if state.choices.take((False, True)):
        _enum(state, schema)
