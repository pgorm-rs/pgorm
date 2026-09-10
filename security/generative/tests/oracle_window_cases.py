"""Window peers, NULL inputs, empty frames and sticky unpartitioned order."""

from execution_cases import Case
from execution_pipeline import source


def window_case(mode, partitioned, *, only=None):
    c = Case()
    query = source(c)
    identity = c.node("pipeline.column", data={"source": "a", "column": "id"})
    score = c.node("pipeline.column", data={"source": "a", "column": "score"})
    partition = []
    if partitioned:
        partition = [
            c.node("pipeline.column", data={"source": "a", "column": "tenant"})
        ]
    key = score if mode == "rank" else identity
    order = c.node("pipeline.unary", {"value": key}, {"operator": "asc"})
    functions = (
        ("rank", "rank_dense")
        if mode == "rank"
        else (
            "sum",
            "min",
            "max",
            "average",
            "count",
            "first",
            "last",
        )
    )
    if only is not None:
        functions = only
    outputs = []
    for name in functions:
        function = c.node("pipeline.function", {"arguments": [score]}, {"name": name})
        outputs.append(
            c.node("pipeline.named", {"value": function}, {"name": "v_" + name})
        )
    frame = {"start": 1, "end": 1} if mode == "empty-frame" else {}
    query = c.node(
        "pipeline.window",
        {"query": query, "columns": outputs, "partition": partition, "order": [order]},
        frame,
    )
    outputs = [
        c.node("pipeline.alias", data={"name": "v_" + name}) for name in functions
    ]
    query = c.node("pipeline.select", {"query": query, "columns": [identity, *outputs]})
    if partitioned or mode == "rank":
        key = c.node("pipeline.alias", data={"name": "id"})
        query = c.node("pipeline.sort", {"query": query, "keys": [key]})
    c.step("fetch", {"query": query}, {"mode": "all", "ordered": True})
    return c.program()


def cases():
    return [
        (f"window-{mode}-partition-{partitioned}", window_case(mode, partitioned))
        for mode in ("rank", "values", "empty-frame")
        for partitioned in (False, True)
    ] + [
        ("known-nullable-count", window_case("values", False, only=("count",))),
        (
            "known-first-last-frame",
            window_case("empty-frame", False, only=("first", "last")),
        ),
    ]
