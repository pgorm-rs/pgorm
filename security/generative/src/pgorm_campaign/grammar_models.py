"""Generate runtime descriptors and the actual registered entity/graph paths."""

from dataclasses import replace

from . import baseline
from .corpus_builtin import ENUM
from .grammar_expr import predicate
from .grammar_sequence import account_row
from .grammar_state import Field, Source

GRAPHS = ("AccountOnly", "OptionalNotes", "RequiredNotes", "SelfJoin") + tuple(
    "Arity" + str(n) for n in range(3, 8)
)


def fields(state, table, *, optional=False):
    definition = next(
        item
        for item in state.author.fixture["tables"]
        if item["schema"] == "fixture" and item["name"] == table
    )
    return tuple(
        Field(column["name"], column["kind"], optional or column["nullable"])
        for column in definition["columns"]
        if isinstance(column["kind"], str) and column["name"] != "tags"
    )


def entity_source(state, name):
    node = state.node("entity", data={"name": "campaign." + name})
    return Source(
        node,
        name,
        fields(state, "accounts" if name == "Account" else "notes"),
        "entity",
    )


def _equal(state, source, column, value):
    return state.node(
        "entity.predicate",
        {"entity": source.node, "value": value},
        {"column": column, "operator": "eq"},
    )


def entity(state):
    name = state.choices.take(("Account", "Note"))
    source = entity_source(state, name)
    query = state.node("entity.find", {"entity": source.node})
    tenant = _equal(state, source, "tenant", state.value("i32", 1))
    query = state.node("entity.filter", {"query": query, "predicate": tenant})
    for _ in range(state.choices.integer(1, 2)):
        other = predicate(state, [source], state.limits.depth)
        query = state.node("entity.filter", {"query": query, "predicate": other.node})
    if name == "Account" and state.choices.take((False, True)):
        value = state.value(ENUM, state.choices.take(("calm", "O'Brien 雪")))
        query = state.node(
            "entity.filter",
            {"query": query, "predicate": _equal(state, source, "state", value)},
        )
    nonce = state.binary(
        state.column(source, "id"), state.constant("i32", state.index + 100), "lt"
    )
    query = state.node("entity.filter", {"query": query, "predicate": nonce.node})
    keys = [
        state.node(
            "expr.order",
            {"value": state.column(source, name).node},
            {"direction": state.choices.take(("asc", "desc")), "nulls": "default"},
        )
        for name in (("rank", "id") if source.alias == "Account" else ("id",))
    ]
    query = state.node("entity.order", {"query": query, "keys": keys})
    query = state.node(
        "entity.page",
        {"query": query},
        {"limit": state.choices.integer(0, 8), "offset": state.choices.integer(0, 2)},
    )
    state.fetch(query)
    if state.choices.take((False, True)):
        state.fetch(query)


def _active_insert(state, source, scope):
    if source.alias == "Account":
        # This registered entity intentionally omits tags; the declared fixture
        # admits that omitted column while retaining its existing array rows.
        next(
            column
            for column in state.author.fixture["tables"][0]["columns"]
            if column["name"] == "tags"
        )["nullable"] = True
        columns, values = account_row(state, 100 + state.index)
        assignments = [
            (column, value)
            for column, value in zip(columns, values, strict=True)
            if column != "tags"
        ]
    else:
        assignments = [
            (column, state.value(kind, value))
            for column, kind, value in (
                ("id", "i32", 100 + state.index),
                ("account_id", "i32", 1),
                ("tenant", "i32", 1),
                ("body", "text", state.text()),
            )
        ]
    active = state.node("entity.active", {"entity": source.node})
    for column, value in assignments:
        active = state.node(
            "active.set",
            {"model": active, "value": value},
            {"column": column, "state": "set"},
        )
    return state.author.effect(
        "active.write", {"model": active}, {"method": "insert"}, scope=scope
    )


def _active_read(state, source, scope):
    identity = state.choices.take(
        (1, 2, 4) if source.alias == "Account" else (11, 12, 14)
    )
    query = state.node("entity.find", {"entity": source.node})
    query = state.node(
        "entity.filter",
        {
            "query": query,
            "predicate": _equal(state, source, "id", state.value("i32", identity)),
        },
    )
    return state.fetch(query, scope=scope)


def active(state):
    source = entity_source(state, state.choices.take(("Account", "Note")))
    scope = "root"
    if state.choices.take((False, True)):
        state.author.effect(
            "begin",
            data={"child": "tx", "mode": "read_write", "isolation": "read_committed"},
        )
        scope = "tx"
    step = (
        _active_insert(state, source, scope)
        if state.choices.take((False, True))
        else _active_read(state, source, scope)
    )
    for _ in range(state.choices.integer(1, 3)):
        model = state.node("entity.result", data={"step": step, "row": 0})
        active = state.node("entity.into_active", {"model": model})
        column = "name" if source.alias == "Account" else "body"
        choice = state.choices.take(("set", "reset", "not_set"))
        inputs = {"model": active}
        if choice == "set":
            inputs["value"] = state.value("text", str(state.index) + state.text())
        active = state.node("active.set", inputs, {"column": column, "state": choice})
        if source.alias == "Account" and state.choices.take((False, True)):
            active = state.node(
                "active.set",
                {"model": active, "value": state.value("text", None, sql_null=True)},
                {"column": "note", "state": "set"},
            )
        step = state.author.effect(
            "active.write", {"model": active}, {"method": "update"}, scope=scope
        )
    if state.choices.take((False, True)):
        model = state.node("entity.result", data={"step": step, "row": 0})
        active = state.node("entity.into_active", {"model": model})
        state.author.effect(
            "active.write", {"model": active}, {"method": "delete"}, scope=scope
        )
    if scope != "root":
        state.author.effect(state.choices.take(("commit", "rollback")), scope=scope)


# [spec:pgorm:req:generative.grammar]
def graph(state, *, cursor=False):
    name = state.choices.take(GRAPHS)
    arity = (
        1 if name == "AccountOnly" else int(name[-1]) if name.startswith("Arity") else 2
    )
    aliases = [state.name("slot" + str(index)) for index in range(1, arity)]
    registration = state.node("graph", data={"name": "campaign." + name})
    query = state.node("graph.find", {"graph": registration}, {"aliases": aliases})
    sources = [Source(query, "Account", fields(state, "accounts"), "graph")]
    for index, alias in enumerate(aliases, 1):
        table = "accounts" if name == "SelfJoin" else "notes"
        sources.append(
            Source(
                query,
                alias,
                fields(state, table, optional=name != "RequiredNotes"),
                "graph",
                index,
            )
        )
    if state.choices.take((False, True)):
        other = predicate(state, sources, state.limits.depth)
        query = state.node("graph.filter", {"query": query, "predicate": other.node})
    nonce = state.binary(
        state.column(sources[0], "id"), state.constant("i32", state.index + 100), "lt"
    )
    query = state.node("graph.filter", {"query": query, "predicate": nonce.node})
    if cursor:
        column = state.choices.take(("id", "rank"))
        query = state.node("graph.cursor", {"query": query}, {"column": column})
        for side in state.choices.take(
            ((), ("before",), ("after",), ("before", "after"))
        ):
            length = arity + int(column != "id")
            values = [
                state.value("i32", state.choices.integer(0, 14)) for _ in range(length)
            ]
            query = state.node(
                "cursor.bound", {"cursor": query, "values": values}, {"side": side}
            )
        query = state.node(
            "cursor.page",
            {"cursor": query},
            {
                "side": state.choices.take(("first", "last")),
                "count": state.choices.integer(1, 5),
                "direction": state.choices.take(("asc", "desc")),
            },
        )
    else:
        keys = [
            state.node(
                "expr.order",
                {"value": state.column(source, "id").node},
                {"direction": state.choices.take(("asc", "desc")), "nulls": "default"},
            )
            for source in sources
        ]
        query = state.node("graph.order", {"query": query, "keys": keys})
    state.fetch(query)


def cursor(state):
    graph(state, cursor=True)


def model(state):
    table_name = state.choices.take(("accounts", "notes", "composite"))
    if table_name == "composite":
        state.author.fixture["tables"].append(
            {
                "schema": "fixture",
                "name": "composite",
                "columns": [
                    baseline.column("id", "i32", primary=True),
                    baseline.column("tenant", "i32", primary=True),
                    baseline.column("name", "text", nullable=True),
                ],
                "rows": [[1, 1, "one"], [1, 2, "other tenant"], [2, 1, None]],
            }
        )
    physical = state.source(table_name)
    selected = [field for field in physical.fields if field.kind in ("i32", "text")]
    mapping = {
        state.name("field" + str(index)): field.name
        for index, field in enumerate(selected)
    }
    descriptor = state.node("model", {"table": physical.node}, {"fields": mapping})
    source = Source(
        descriptor,
        physical.alias,
        tuple(
            replace(field, name=logical)
            for logical, field in zip(mapping, selected, strict=True)
        ),
        "model",
    )
    test = predicate(state, [source], state.limits.depth)
    query = state.node(
        "model.select", {"model": descriptor, "predicate": test.node, "order": []}
    )
    state.fetch(query)
    method = state.choices.take(
        (None, "update", "delete")
        if table_name == "accounts"
        else (None, "insert", "update", "delete")
    )
    if method is not None:
        column = next(logical for logical, real in mapping.items() if real == "tenant")
        tenant = state.binary(
            state.column(source, column), state.constant("i32", 1), "eq"
        )
        text_column = next(
            logical for logical, real in mapping.items() if real in ("name", "body")
        )
        inputs, columns = {"model": descriptor, "values": []}, []
        if method != "insert":
            inputs["predicate"] = tenant.node
        if method == "update":
            columns = [text_column]
            inputs["values"] = [state.value("text", str(state.index) + state.text())]
        elif method == "insert":
            # Public runtime-model inserts require an unaliased table. Reads,
            # updates and deletes retain the independently generated alias.
            table = state.node("table", data={"schema": "fixture", "name": table_name})
            inputs["model"] = state.node("model", {"table": table}, {"fields": mapping})
            for logical, field in zip(mapping, selected, strict=True):
                if field.nullable and state.choices.take((False, True)):
                    continue
                null = field.nullable and state.choices.take((False, True))
                data = (
                    state.text()
                    if field.kind == "text"
                    else state.index + 100
                    if field.name == "id"
                    else 1
                )
                columns.append(logical)
                inputs["values"].append(
                    state.value(field.kind, None if null else data, sql_null=null)
                )
        write = state.node(
            "model.write",
            inputs,
            {"method": method, "columns": columns},
        )
        write = state.node(
            "model.returning", {"query": write}, {"columns": list(mapping)}
        )
        state.fetch(write)
        state.fetch(query)
