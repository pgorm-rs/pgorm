"""Protected native baselines for closed, fixture-owned sensitivity controls."""

from .author import Author
from . import wire

PAYLOAD = "' OR TRUE --"
IDENTIFIER = 'found" FROM fixture.accounts WHERE TRUE --'


def read(mode="rows"):
    a = Author()
    table = a.node("table", data={"schema": "fixture", "name": "accounts"})
    identity = a.node("expr.column", {"table": table}, {"name": "id"})
    tenant = a.node("expr.column", {"table": table}, {"name": "tenant"})
    predicate = a.node(
        "expr.binary", {"left": tenant, "right": a.value("i32", 1)}, {"operator": "eq"}
    )
    if mode in ("escaped-value", "bind-order"):
        column = (
            a.node("expr.column", {"table": table}, {"name": "name"})
            if mode == "escaped-value"
            else identity
        )
        value = (
            a.value("text", PAYLOAD) if mode == "escaped-value" else a.value("i32", 2)
        )
        other = a.node(
            "expr.binary", {"left": column, "right": value}, {"operator": "eq"}
        )
        predicate = a.node("condition", {"items": [predicate, other]}, {"mode": "all"})
    projection = (
        a.node("expr.alias", {"value": identity}, {"name": IDENTIFIER})
        if mode == "identifier"
        else identity
    )
    query = a.node("select", {"columns": [projection]})
    query = a.node("select.from", {"query": query, "table": table})
    query = a.node("select.filter", {"query": query, "predicate": predicate})
    key = a.node(
        "expr.order", {"value": identity}, {"direction": "asc", "nulls": "default"}
    )
    query = a.node("select.order", {"query": query, "keys": [key]})
    if mode == "stream":
        a.effect(
            "stream", {"query": query}, {"take": 2, "cancel": False, "ordered": True}
        )
    else:
        a.fetch(query, ordered=True)
    return a.finish()


def typed(kind, data, postgres):
    a = Author()
    value = a.value(kind, data)
    expression = a.node("expr.value", {"value": value}, {"mode": "bound"})
    expression = a.node(
        "expr.cast", {"value": expression}, {"schema": "pg_catalog", "name": postgres}
    )
    expression = a.node("expr.alias", {"value": expression}, {"name": "value"})
    a.fetch(a.node("select", {"columns": [expression]}))
    return a.finish()


def structure(mode):
    a = Author()
    if mode == "optional":
        graph = a.node("graph", data={"name": "campaign.OptionalNotes"})
        query = a.node("graph.find", {"graph": graph}, {"aliases": ["n"]})
    else:
        table = a.node("table", data={"schema": "fixture", "name": "accounts"})
        column = a.node(
            "expr.column",
            {"table": table},
            {"name": "tags" if mode == "array" else "state"},
        )
        query = a.node("select", {"columns": [column]})
        query = a.node("select.from", {"query": query, "table": table})
    a.fetch(query)
    return a.finish()


def write(*, rollback=False):
    a = Author()
    table = a.node("table", data={"schema": "fixture", "name": "accounts"})
    column = a.node("expr.column", {"table": table}, {"name": "tenant"})
    predicate = a.node(
        "expr.binary", {"left": column, "right": a.value("i32", 1)}, {"operator": "eq"}
    )
    query = a.node("update", {"table": table})
    query = a.node(
        "update.set",
        {"query": query, "value": a.value("text", "control changed")},
        {"column": "name"},
    )
    query = a.node("write.filter", {"query": query, "predicate": predicate})
    if rollback:
        a.effect(
            "begin",
            data={"child": "tx", "mode": "read_write", "isolation": "read_committed"},
        )
    a.effect("execute", {"query": query}, scope="tx" if rollback else "root")
    if rollback:
        a.effect("rollback", scope="tx")
    return a.finish()


def schema():
    a = Author()
    table = a.node("table", data={"schema": "fixture", "name": "control_created"})
    query = a.node(
        "schema.create",
        {"table": table},
        {
            "columns": [
                {"name": "id", "kind": "i32", "primary": True, "nullable": False}
            ]
        },
    )
    a.effect("execute", {"query": query})
    return a.finish()


def rejection():
    a = Author()
    query = a.node(
        "raw.template",
        {"parameters": [a.value("i32", 1), a.value("i32", 0)]},
        {"text": "SELECT $1::int4 / $2::int4 AS value"},
    )
    a.effect(
        "fetch",
        {"query": query},
        {"mode": "all", "ordered": False},
        error={"class": "DatabaseError", "cause": "sqlstate:22012"},
    )
    return a.finish()


def numeric(kind):
    sample, postgres = {
        "float": (wire.scalar("f32", "80000000"), "float4"),
        "decimal": (wire.scalar("decimal", "123.4500"), "numeric"),
        "temporal": (
            wire.scalar("datetime", "2024-01-02 03:04:05.123456"),
            "timestamp",
        ),
    }[kind]
    return typed(sample["type"], sample["data"], postgres)
