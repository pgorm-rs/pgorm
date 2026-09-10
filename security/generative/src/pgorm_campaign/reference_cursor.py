"""Lexicographic PostgreSQL row comparisons for declared graph cursor keys."""

from dataclasses import replace

from .comparison import InvalidOracle
from .reference_sql import bound, join


def cursor_node(name, i, d):
    if name == "graph.cursor":
        query = i["query"]
        slots = query.shape["slots"]
        keys = [slots[0].column(d["column"])]
        keys.extend(
            slot.column("id")
            for index, slot in enumerate(slots)
            if index or d["column"] != "id"
        )
        return replace(
            query,
            shape={
                **query.shape,
                "cursor": {
                    "keys": keys,
                    "bounds": {},
                    "direction": "asc",
                    "side": "first",
                    "count": None,
                },
            },
        )
    query = i["cursor"]
    cursor = query.shape["cursor"]
    if name == "cursor.bound":
        if len(i["values"]) not in (1, len(cursor["keys"])):
            raise InvalidOracle("cursor oracle needs every declared key component")
        cursor = {**cursor, "bounds": {**cursor["bounds"], d["side"]: i["values"]}}
    elif name == "cursor.page":
        cursor = {**cursor, **d}
    else:
        raise InvalidOracle("independent cursor semantics uncovered: " + name)
    return replace(query, shape={**query.shape, "cursor": cursor})


def prepare(query):
    cursor = query.shape["cursor"]
    filters = list(query.filters)
    for side, values in cursor["bounds"].items():
        greater = (side == "after") == (cursor["direction"] == "asc")
        filters.append(
            "("
            + join(cursor["keys"][: len(values)])
            + (") > (" if greater else ") < (")
            + join([bound(value) for value in values])
            + ")"
        )
    descending = (cursor["direction"] == "desc") != (cursor["side"] == "last")
    return replace(
        query,
        filters=tuple(filters),
        order=tuple(
            key + (" DESC" if descending else " ASC") for key in cursor["keys"]
        ),
        limit=cursor["count"],
        offset=None,
    )
