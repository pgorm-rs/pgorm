"""Ordered compositions with keys that survive, disappear or cross a binding."""

from execution_cases import Case
from execution_pipeline import source


def ordered(mode):
    c = Case()
    query = source(c)
    identity = c.node("pipeline.column", data={"source": "a", "column": "id"})
    name = c.node("pipeline.column", data={"source": "a", "column": "name"})
    key = c.node("pipeline.unary", {"value": identity}, {"operator": "desc"})
    keys = [key]
    if mode == "nulls":
        score = c.node("pipeline.column", data={"source": "a", "column": "score"})
        keys.insert(0, c.node("pipeline.unary", {"value": score}, {"operator": "desc"}))
    query = c.node("pipeline.sort", {"query": query, "keys": keys})
    if mode == "join":
        table = c.node("table", data={"schema": "fixture", "name": "notes"})
        notes = c.node("pipeline.source", {"source": table}, {"alias": "n"})
        foreign = c.node(
            "pipeline.column", data={"source": "n", "column": "account_id"}
        )
        predicate = c.node(
            "pipeline.binary", {"left": identity, "right": foreign}, {"operator": "eq"}
        )
        query = c.node(
            "pipeline.join",
            {"query": query, "source": notes, "on": predicate},
            {"kind": "left"},
        )
        query = c.node("pipeline.select", {"query": query, "columns": [identity]})
    elif mode == "hidden":
        lower = c.node("pipeline.value", {"value": c.value("i32", 1)})
        predicate = c.node(
            "pipeline.binary", {"left": identity, "right": lower}, {"operator": "gt"}
        )
        query = c.node("pipeline.filter", {"query": query, "predicate": predicate})
        query = c.node("pipeline.select", {"query": query, "columns": [name]})
        query = c.node("pipeline.take", {"query": query}, {"start": 2, "end": 3})
    elif mode == "sources":
        query = c.node(
            "pipeline.sources",
            {"query": query},
            {"name": "campaign.Sources1", "qualifiers": ["a"]},
        )
    else:
        query = c.node("pipeline.select", {"query": query, "columns": [identity]})
        if mode == "distinct":
            query = c.node("pipeline.distinct", {"query": query})
            identity = c.node("pipeline.alias", data={"name": "id"})
            key = c.node("pipeline.unary", {"value": identity}, {"operator": "desc"})
            query = c.node("pipeline.sort", {"query": query, "keys": [key]})
        elif mode == "embedded":
            query = c.node("pipeline.take", {"query": query}, {"start": 2, "end": 3})
            query = c.node("pipeline.source", {"source": query}, {"alias": "nested"})
            query = c.node("pipeline.from", {"source": query})
            identity = c.node(
                "pipeline.column", data={"source": "nested", "column": "id"}
            )
            query = c.node("pipeline.select", {"query": query, "columns": [identity]})
    # All authored nodes must be reached; name is used only by the hidden-key case.
    if mode != "hidden":
        c.nodes = [node for node in c.nodes if node["id"] != name]
    c.step("fetch", {"query": query}, {"mode": "all", "ordered": True})
    return c.program()


def cases():
    return [
        ("ordered-pipeline-" + mode, ordered(mode))
        for mode in (
            "hidden",
            "distinct",
            "embedded",
            "nulls",
            "sources",
            "join",
        )
    ]
