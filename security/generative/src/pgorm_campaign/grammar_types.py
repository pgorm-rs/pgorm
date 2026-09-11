"""Generate bound/literal type contexts without erasing exact input tags."""

from functools import lru_cache
import math
import struct

from . import matrix, wire
from .corpus_random import sample
from .grammar_state import DEFAULT_INPUTS

ROWS = {row["kind"]: row for row in matrix.load()["value_matrix"]}
LIVE = tuple(kind for kind, row in ROWS.items() if row["policy"] == "live")


def compatible(value, *, literal=False):
    if value["sql_null"]:
        return True
    kind, data = value["type"]["kind"], value["data"]
    if kind in ("text", "char", "enum"):
        return "\0" not in data
    if kind == "json":
        return not json_nul(data)
    if kind == "array":
        return all(compatible(item, literal=literal) for item in data)
    if literal and kind in ("f32", "f64"):
        return math.isfinite(
            struct.unpack(">f" if kind == "f32" else ">d", bytes.fromhex(data))[0]
        )
    return kind not in ("u64", "vector")


def json_nul(value):
    if isinstance(value, str):
        return "\0" in value
    if isinstance(value, dict):
        return any(json_nul(key) or json_nul(item) for key, item in value.items())
    return isinstance(value, list) and any(json_nul(item) for item in value)


@lru_cache(maxsize=1)
def candidates():
    return {
        kind: tuple(
            item
            for item in DEFAULT_INPUTS
            if item.value()["type"]["kind"] == kind
            and not item.value()["sql_null"]
            and compatible(item.value())
        )
        for kind in LIVE
    }


def contexts(value):
    tag = value["type"]
    array = {"kind": "array", "element": tag}
    null = wire.scalar(tag, None, sql_null=True)
    return (
        value,
        null,
        wire.scalar(array, []),
        wire.scalar(array, None, sql_null=True),
        wire.scalar(array, [value, null, value]),
    )


def cast(state, expression, value):
    tag = value["type"]
    array = tag["kind"] == "array"
    tag = tag["element"] if array else tag
    if tag["kind"] == "enum":
        name, schema = tag["name"], tag["schema"]
    else:
        schema, name = ROWS[tag["kind"]]["postgres"].split(".")
    if tag["kind"] == "i8":
        expression = state.node(
            "expr.cast",
            {"value": expression},
            {"name": "int4", "schema": "pg_catalog", "array": array},
        )
    return state.node(
        "expr.cast",
        {"value": expression},
        {"name": name, "schema": schema, "array": array},
    )


# [spec:pgorm:req:generative.grammar]
def types(state):
    kind = state.choices.take(LIVE)
    selected = state.choices.take(candidates()[kind])
    random = sample(state.seed, state.index)
    if random.value()["type"]["kind"] == kind and compatible(random.value()):
        selected = random
    state.used_inputs.add(selected.data()["id"])
    values, columns = {}, []
    for mode in state.choices.take((("bound", "literal"), ("literal", "bound"))):
        item = selected
        if mode == "literal" and not compatible(item.value(), literal=True):
            item = state.choices.take(
                tuple(
                    item
                    for item in candidates()[kind]
                    if compatible(item.value(), literal=True)
                )
            )
            state.used_inputs.add(item.data()["id"])
        for index, value in enumerate(contexts(item.value())):
            key = repr(value)
            if key not in values:
                values[key] = state.node("value", data={"value": value})
            expression = state.node(
                "expr.value", {"value": values[key]}, {"mode": mode}
            )
            expression = cast(state, expression, value)
            columns.append(
                state.node(
                    "expr.alias",
                    {"value": expression},
                    {"name": state.name(mode + str(index))},
                )
            )
    columns.append(
        state.node(
            "expr.alias",
            {"value": state.constant("i64", state.index).node},
            {"name": "nonce"},
        )
    )
    query = state.node("select", {"columns": columns})
    state.fetch(query)
    if state.choices.take((False, True)):
        state.author.effect("inspect", {"query": query})
