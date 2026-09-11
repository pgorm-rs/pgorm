"""Grouped, windowed, set and compiled-source pipeline compositions."""

from .grammar_pipeline import Column, Pipeline
from .grammar_state import Expression


def grouped(state):
    source = state.source()
    key = state.column(source, state.choices.take(("tenant", "rank", "name")))
    value = state.column(source, state.choices.take(("id", "score")))
    count = state.node("expr.call", {"arguments": [value.node]}, {"name": "count"})
    columns = [
        state.node("expr.alias", {"value": key.node}, {"name": state.name("key")}),
        state.node("expr.alias", {"value": count}, {"name": state.name("count")}),
        state.constant("i64", state.index).node,
    ]
    query = state.node("select", {"columns": columns})
    query = state.node("select.from", {"query": query, "table": source.node})
    query = state.node("select.group", {"query": query, "keys": [key.node]})
    limit = state.constant("i64", state.choices.integer(0, 3))
    having = state.binary(
        Expression(count, "i64", False), limit, state.choices.take(("gt", "gte", "eq"))
    )
    query = state.node("select.having", {"query": query, "predicate": having.node})
    state.fetch(query)


def _group(query):
    state = query.state
    key = state.choices.take(
        [column for column in query.columns if column.kind in ("text", "i32")]
    )
    argument = state.choices.take(
        # PRQL group passes the relation excluding its keys into the aggregate
        # body (prqlc semantic/resolver/transforms.rs, group partition). Keys
        # become available again on the resulting grouped relation.
        [column for column in query.columns if column.kind == "i32" and column != key]
    )
    key_expr, argument_expr = query.expression(key), query.expression(argument)
    query.stage("pipeline.group", {"keys": [key_expr]})
    function = state.choices.take(
        ("sum", "min", "max", "average", "count", "count_distinct", "count_rows")
    )
    value = state.node(
        "pipeline.function",
        {"arguments": [] if function == "count_rows" else [argument_expr]},
        {"name": function},
    )
    name = state.name("aggregate")
    value = state.node("pipeline.named", {"value": value}, {"name": name})
    query.stage("pipeline.aggregate", {"columns": [value]})
    kind = (
        "i32"
        if function in ("min", "max")
        else "decimal"
        if function == "average"
        else "i64"
    )
    query.columns = [
        Column(key.name, key.kind, key.nullable),
        Column(name, kind, function not in ("count", "count_distinct", "count_rows")),
    ]
    # HAVING is represented by a filter after the grouped aggregate relation.
    value = query.expression(query.columns[0])
    bound = query.constant(
        key.kind,
        state.text() if key.kind == "text" else state.choices.integer(0, 3),
        "root",
    )
    predicate = state.node(
        "pipeline.binary",
        {"left": value, "right": bound},
        {"operator": state.choices.take(("eq", "ne", "gte"))},
    )
    query.stage("pipeline.filter", {"predicate": predicate})


def _window(query):
    state = query.state
    argument = state.choices.take(
        [column for column in query.columns if column.kind == "i32"]
    )
    function = state.choices.take(
        ("row_number", "rank", "rank_dense", "count", "sum", "first", "last")
    )
    arguments = [] if function == "row_number" else [query.expression(argument)]
    value = state.node(
        "pipeline.function", {"arguments": arguments}, {"name": function}
    )
    name = state.name("window")
    value = state.node("pipeline.named", {"value": value}, {"name": name})
    key = next(column for column in query.columns if column.name == "p_id")
    order = state.node(
        "pipeline.unary",
        {"value": query.expression(key)},
        {"operator": state.choices.take(("asc", "desc"))},
    )
    partition = (
        [
            query.expression(
                # Partitioned windows use PRQL group too: the partition key
                # is outside the window body's expression/order source scope.
                state.choices.take(
                    [
                        column
                        for column in query.columns
                        if column.kind == "i32" and column not in (argument, key)
                    ]
                )
            )
        ]
        if state.choices.take((False, True))
        else []
    )
    options = state.choices.take(({}, {"start": -1, "end": 0}, {"start": 1, "end": 1}))
    query.stage(
        "pipeline.window",
        {"columns": [value], "partition": partition, "order": [order]},
        data=options,
    )
    query.columns.append(
        Column(
            name,
            argument.kind if function in ("first", "last") else "i64",
            function in ("first", "last", "sum"),
        )
    )


# [spec:pgorm:req:generative.grammar]
def relational(state):
    query = Pipeline(state)
    if state.choices.take((False, True)):
        query.filter()
    if state.choices.take((False, True)):
        _group(query)
    else:
        _window(query)
    if state.choices.take((False, True)):
        query.nest()
    query.project()
    state.fetch(query.query)


def sets(state):
    query = Pipeline(state)
    query.filter()
    if state.choices.take((False, True)):
        query.distinct()
    original = query.query
    for _ in range(state.choices.integer(1, state.limits.stages)):
        other = original
        if state.choices.take((False, True)):
            other = state.node("pipeline.distinct", {"query": original})
        query.stage(
            "pipeline.set",
            {"source": other},
            data={"method": state.choices.take(("append", "intersect", "remove"))},
        )
        if state.choices.take((False, True)):
            query.distinct()
        state.fetch(query.query)


def sources(state):
    arity = state.choices.integer(1, 6)
    aliases = [state.name("source" + str(index)) for index in range(arity)]
    entity = state.node("entity", data={"name": "campaign.Account"})
    source = state.node("pipeline.source", {"source": entity}, {"alias": aliases[0]})
    query = state.node("pipeline.from", {"source": source})
    root = state.node("pipeline.column", data={"source": aliases[0], "column": "id"})
    nonce = state.node(
        "pipeline.value", {"value": state.value("i32", state.index + 100)}
    )
    test = state.node(
        "pipeline.binary", {"left": root, "right": nonce}, {"operator": "lt"}
    )
    query = state.node("pipeline.filter", {"query": query, "predicate": test})
    for alias in aliases[1:]:
        entity = state.node("entity", data={"name": "campaign.Note"})
        source = state.node("pipeline.source", {"source": entity}, {"alias": alias})
        other = state.node(
            "pipeline.column", data={"source": alias, "column": "account_id"}
        )
        test = state.node(
            "pipeline.binary", {"left": root, "right": other}, {"operator": "eq"}
        )
        query = state.node(
            "pipeline.join",
            {"query": query, "source": source, "on": test},
            {"kind": state.choices.take(("inner", "left", "right", "full"))},
        )
    if state.choices.take((False, True)):
        items = [
            state.node("pipeline.value", {"value": state.value("i32", n)})
            for n in range(state.choices.integer(0, 5))
        ]
        test = state.node("pipeline.membership", {"value": root, "items": items})
        query = state.node("pipeline.filter", {"query": query, "predicate": test})
    query = state.node(
        "pipeline.sources",
        {"query": query},
        {"name": "campaign.Sources" + str(arity), "qualifiers": aliases},
    )
    state.fetch(query)


def template(state):
    value = state.value("text", state.text())
    nonce = state.value("i64", state.index)
    comment = state.choices.take(
        (" /* $3 ' comment */", " -- $4\n", " /* outer /* $5 */ comment */")
    )
    literal = state.choices.take(("'$1'", "$tag$$2'/*$tag$", "'quote''$2'"))
    text = (
        "SELECT $1::text AS value, $2::bigint AS nonce, $1::text AS repeated, "
        + literal
        + " AS literal"
        + comment
    )
    query = state.node("raw.template", {"parameters": [value, nonce]}, {"text": text})
    state.fetch(query)
    if state.choices.take((False, True)):
        state.author.effect("inspect", {"query": query})
