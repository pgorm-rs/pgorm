"""Exercise concrete stage composition, callbacks and compiled source tuples."""

from execution_cases import Case


def source(c, name="accounts", alias="a"):
    table = c.node("table", data={"schema": "fixture", "name": name})
    named = c.node("pipeline.source", {"source": table}, {"alias": alias})
    return c.node("pipeline.from", {"source": named})


def stages():
    c = Case()
    query = source(c)
    column = c.node("pipeline.column", data={"source": "a", "column": "id"})
    items = [c.node("pipeline.value", {"value": c.value("i32", i)}) for i in (1, 2)]
    membership = c.node("pipeline.membership", {"value": column, "items": items})
    query = c.node("pipeline.filter", {"query": query, "predicate": membership})
    bound = c.node("pipeline.bind", {"value": c.value("i32", 1)}, scope="b")
    value = c.node(
        "pipeline.binary",
        {"left": column, "right": bound},
        {"operator": "add"},
        scope="b",
    )
    value = c.node("pipeline.cast", {"value": value}, {"name": "bigint"}, scope="b")
    value = c.node("pipeline.named", {"value": value}, {"name": "next"}, scope="b")
    query = c.node(
        "pipeline.derive", {"query": query, "columns": [value]}, {"binder": "b"}
    )
    c.binders.append({"id": "b", "owner": query})
    value = c.node("pipeline.alias", data={"name": "next"})
    negative = c.node("pipeline.unary", {"value": value}, {"operator": "neg"})
    projection = c.node("pipeline.named", {"value": negative}, {"name": "result"})
    query = c.node("pipeline.select", {"query": query, "columns": [projection]})
    alias = c.node("pipeline.alias", data={"name": "result"})
    key = c.node("pipeline.unary", {"value": alias}, {"operator": "desc"})
    query = c.node("pipeline.sort", {"query": query, "keys": [key]})
    query = c.node("pipeline.take", {"query": query}, {"start": 1, "end": 2})
    query = c.node("pipeline.distinct", {"query": query})
    c.fetch(query)
    return c.program()


def sets():
    c = Case()
    query = source(c)
    column = c.node("pipeline.column", data={"source": "a", "column": "id"})
    original = c.node("pipeline.select", {"query": query, "columns": [column]})
    query = original
    for method in ("append", "intersect", "remove"):
        query = c.node(
            "pipeline.set", {"query": original, "source": original}, {"method": method}
        )
        c.fetch(query)
    return c.program()


def group():
    c = Case()
    query = source(c)
    tenant = c.node("pipeline.column", data={"source": "a", "column": "tenant"})
    identity = c.node("pipeline.column", data={"source": "a", "column": "id"})
    grouped = c.node("pipeline.group", {"query": query, "keys": [tenant]})
    summed = c.node("pipeline.function", {"arguments": [identity]}, {"name": "sum"})
    column = c.node("pipeline.named", {"value": summed}, {"name": "total"})
    query = c.node("pipeline.aggregate", {"query": grouped, "columns": [column]})
    alias = c.node("pipeline.alias", data={"name": "total"})
    limit = c.node("pipeline.value", {"value": c.value("i32", 3)})
    predicate = c.node(
        "pipeline.binary", {"left": alias, "right": limit}, {"operator": "gt"}
    )
    query = c.node("pipeline.filter", {"query": query, "predicate": predicate})
    c.fetch(query)
    return c.program()


def window():
    c = Case()
    query = source(c)
    tenant = c.node("pipeline.column", data={"source": "a", "column": "tenant"})
    identity = c.node("pipeline.column", data={"source": "a", "column": "id"})
    key = c.node("pipeline.unary", {"value": identity}, {"operator": "asc"})
    rank = c.node("pipeline.function", {"arguments": []}, {"name": "row_number"})
    rank = c.node("pipeline.named", {"value": rank}, {"name": "number"})
    query = c.node(
        "pipeline.window",
        {"query": query, "columns": [rank], "partition": [tenant], "order": [key]},
        {"start": -1, "end": 0},
    )
    number = c.node("pipeline.alias", data={"name": "number"})
    query = c.node("pipeline.select", {"query": query, "columns": [identity, number]})
    c.fetch(query)
    return c.program()


def sources(arity):
    c = Case()
    query = source(c)
    identity = c.node("pipeline.column", data={"source": "a", "column": "id"})
    aliases = ["a"]
    for i in range(1, arity):
        alias = "n" + str(i)
        aliases.append(alias)
        table = c.node("table", data={"schema": "fixture", "name": "notes"})
        joined = c.node("pipeline.source", {"source": table}, {"alias": alias})
        foreign = c.node(
            "pipeline.column", data={"source": alias, "column": "account_id"}
        )
        predicate = c.node(
            "pipeline.binary", {"left": identity, "right": foreign}, {"operator": "eq"}
        )
        query = c.node(
            "pipeline.join",
            {"query": query, "source": joined, "on": predicate},
            {"kind": "left"},
        )
    query = c.node(
        "pipeline.sources",
        {"query": query},
        {"name": "campaign.Sources" + str(arity), "qualifiers": aliases},
    )
    c.fetch(query)
    return c.program()
