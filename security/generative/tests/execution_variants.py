"""Additional builder combinations for validating the instruction dispatcher."""

from execution_cases import Case


def expressions():
    c = Case()
    name = c.node("name", {"value": c.value("text", "accounts")})
    table = c.node("table", {"name": name}, {"schema": "fixture", "alias": "a"})
    column = c.node("expr.column", {"table": table}, {"name": "id"})
    membership = c.node(
        "expr.membership",
        {"value": column, "items": [c.value("i32", 1), c.value("i32", 2)]},
        {"negated": False},
    )
    absent = c.node("expr.unary", {"value": column}, {"operator": "is_null"})
    present = c.node("condition", {"items": [absent]}, {"mode": "any", "negated": True})
    predicate = c.node("condition", {"items": [membership, present]}, {"mode": "all"})
    maximum = c.node("expr.call", {"arguments": [column]}, {"name": "max"})
    cast = c.node(
        "expr.cast", {"value": maximum}, {"name": "int8", "schema": "pg_catalog"}
    )
    alias = c.node("name", {"value": c.value("text", 'peak" 雪')})
    projection = c.node("expr.alias", {"value": cast, "name": alias})
    query = c.node("select", {"columns": [projection]})
    query = c.node("select.from", {"query": query, "table": table})
    query = c.node("select.filter", {"query": query, "predicate": predicate})
    query = c.node("select.group", {"query": query, "keys": []})
    literal = c.node("expr.value", {"value": c.value("i32", 0)}, {"mode": "literal"})
    having = c.node(
        "expr.binary", {"left": maximum, "right": literal}, {"operator": "gt"}
    )
    query = c.node("select.having", {"query": query, "predicate": having})
    order = c.node(
        "expr.order", {"value": cast}, {"direction": "desc", "nulls": "last"}
    )
    query = c.node("select.order", {"query": query, "keys": [order]})
    query = c.node("select.distinct", {"query": query})
    query = c.node("select.page", {"query": query}, {"limit": 1, "offset": 0})
    c.step("inspect", {"query": query})
    c.fetch(query)
    return c.program()


def patterns(method):
    c = Case()
    table = c.node("table", data={"name": "accounts", "schema": "fixture"})
    name = c.node("expr.column", {"table": table}, {"name": "name"})
    column = c.node("expr.column", {"table": table}, {"name": "id"})
    text = "%O'Brien%" if method in ("like", "ilike") else "O'Brien"
    predicate = c.node(
        "expr.pattern",
        {"value": name, "pattern": c.value("text", text)},
        {"method": method},
    )
    query = c.node("select", {"columns": [column]})
    query = c.node("select.from", {"query": query, "table": table})
    query = c.node("select.filter", {"query": query, "predicate": predicate})
    c.fetch(query)
    return c.program()


def joins():
    c = Case()
    table = c.node(
        "table", data={"schema": "fixture", "name": "accounts", "alias": "a"}
    )
    notes = c.node("table", data={"schema": "fixture", "name": "notes", "alias": "n"})
    account_id = c.node("expr.column", {"table": table}, {"name": "id"})
    note_id = c.node("expr.column", {"table": notes}, {"name": "id"})
    foreign = c.node("expr.column", {"table": notes}, {"name": "account_id"})
    predicate = c.node(
        "expr.binary", {"left": account_id, "right": foreign}, {"operator": "eq"}
    )
    left = c.node("expr.alias", {"value": account_id}, {"name": "account"})
    right = c.node("expr.alias", {"value": note_id}, {"name": "note"})
    query = c.node("select", {"columns": [left, right]})
    query = c.node("select.from", {"query": query, "table": table})
    query = c.node(
        "select.join",
        {"query": query, "table": notes, "on": predicate},
        {"kind": "inner"},
    )
    c.fetch(query)
    return c.program()


def graph_filters():
    c = Case()
    graph = c.node("graph", data={"name": "campaign.AccountOnly"})
    query = c.node("graph.find", {"graph": graph}, {"aliases": []})
    column = c.node("graph.column", {"query": query}, {"source": 0, "column": "id"})
    predicate = c.node(
        "expr.binary", {"left": column, "right": c.value("i32", 0)}, {"operator": "gt"}
    )
    query = c.node("graph.filter", {"query": query, "predicate": predicate})
    order = c.node(
        "expr.order", {"value": column}, {"direction": "asc", "nulls": "default"}
    )
    query = c.node("graph.order", {"query": query, "keys": [order]})
    c.fetch(query)
    query = c.node("graph.cursor", {"query": query}, {"column": "rank"})
    query = c.node(
        "cursor.bound",
        {"cursor": query, "values": [c.value("i32", 1), c.value("i32", 1)]},
        {"side": "after"},
    )
    query = c.node(
        "cursor.page",
        {"cursor": query},
        {"side": "first", "count": 2, "direction": "asc"},
    )
    c.fetch(query)
    return c.program()


def writes():
    c = Case()
    table = c.node("table", data={"name": "notes", "schema": "fixture"})
    column = c.node("expr.column", {"table": table}, {"name": "id"})
    values = [c.value("i32", v) for v in (80, 1, 1)] + [
        c.value("text", "O'Brien -- 雪")
    ]
    query = c.node(
        "insert", {"table": table}, {"columns": ["id", "account_id", "tenant", "body"]}
    )
    query = c.node("insert.row", {"query": query, "values": values})
    query = c.node(
        "insert.conflict",
        {"query": query},
        {"keys": ["id"], "action": "update", "columns": ["body"]},
    )
    query = c.node("write.returning", {"query": query, "columns": [column]})
    first = c.fetch(query)
    c.fetch(query)
    value = c.node(
        "result.value",
        data={"step": first, "row": 0, "column": "id", "type": {"kind": "i32"}},
    )
    predicate = c.node(
        "expr.binary", {"left": column, "right": value}, {"operator": "eq"}
    )
    query = c.node("delete", {"table": table})
    query = c.node("write.filter", {"query": query, "predicate": predicate})
    c.step("execute", {"query": query})
    model = c.node("model", {"table": table})
    model_column = c.node("model.column", {"model": model}, {"name": "id"})
    predicate = c.node(
        "expr.binary", {"left": model_column, "right": value}, {"operator": "eq"}
    )
    query = c.node(
        "model.write",
        {"model": model, "values": values},
        {"method": "insert", "columns": ["id", "account_id", "tenant", "body"]},
    )
    query = c.node("model.returning", {"query": query}, {"columns": ["id", "body"]})
    c.fetch(query)
    query = c.node(
        "model.write",
        {"model": model, "values": [], "predicate": predicate},
        {"method": "delete", "columns": []},
    )
    c.step("execute", {"query": query})
    query = c.node(
        "raw.template",
        {"parameters": [c.value("text", "O'Brien")]},
        {
            "text": "SELECT $1::text AS value, $1::text AS repeated, '$9'::text AS quoted /* $2 */"
        },
    )
    c.fetch(query)
    return c.program()


def entity_writes():
    c = Case()
    entity = c.node("entity", data={"name": "campaign.Note"})
    active = c.node("entity.active", {"entity": entity})
    for column, kind, value in (
        ("id", "i32", 80),
        ("account_id", "i32", 1),
        ("tenant", "i32", 1),
        ("body", "text", "created"),
    ):
        active = c.node(
            "active.set",
            {"model": active, "value": c.value(kind, value)},
            {"column": column, "state": "set"},
        )
    written = c.step("active.write", {"model": active}, {"method": "insert"})
    model = c.node("entity.result", data={"step": written, "row": 0})
    active = c.node("entity.into_active", {"model": model})
    active = c.node(
        "active.set", {"model": active}, {"column": "body", "state": "reset"}
    )
    c.step("active.write", {"model": active}, {"method": "update"})
    c.step("active.write", {"model": active}, {"method": "delete"})
    predicate = c.node(
        "entity.predicate",
        {"entity": entity, "value": c.value("i32", 80)},
        {"column": "id", "operator": "eq"},
    )
    query = c.node("entity.find", {"entity": entity})
    query = c.node("entity.filter", {"query": query, "predicate": predicate})
    c.fetch(query)
    return c.program()


def schema_changes():
    c = Case()
    query = c.node(
        "schema.enum",
        data={"name": 'transient" 雪', "schema": "fixture", "labels": ["one"]},
    )
    c.step("execute", {"query": query})
    for method, other in (
        ("add", {"value": "two"}),
        ("rename", {"value": "two", "new_value": "O'Brien"}),
        ("drop", {}),
    ):
        query = c.node(
            "schema.enum_change",
            data={
                "name": 'transient" 雪',
                "schema": "fixture",
                "method": method,
                **other,
            },
        )
        c.step("execute", {"query": query})
    table = c.node("table", data={"name": "scratch", "schema": "fixture"})
    query = c.node(
        "schema.create",
        {"table": table},
        {
            "columns": [
                {"name": "id", "kind": "i32", "nullable": True, "primary": False}
            ]
        },
    )
    c.step("execute", {"query": query})
    query = c.node("insert", {"table": table}, {"columns": []})
    query = c.node("insert.defaults", {"query": query})
    c.step("execute", {"query": query})
    query = c.node(
        "schema.index",
        {"table": table},
        {"name": "scratch_ix", "columns": ["id"], "unique": False},
    )
    c.step("execute", {"query": query})
    query = c.node("schema.rename", {"table": table}, {"name": "value", "column": "id"})
    c.step("execute", {"query": query})
    query = c.node("schema.rename", {"table": table}, {"name": "renamed"})
    c.step("execute", {"query": query})
    renamed = c.node("table", data={"name": "renamed", "schema": "fixture"})
    column = c.node("expr.column", {"table": renamed}, {"name": "value"})
    query = c.node("select", {"columns": [column]})
    query = c.node("select.from", {"query": query, "table": renamed})
    c.fetch(query)
    return c.program()
