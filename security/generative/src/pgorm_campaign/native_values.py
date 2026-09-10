"""Materialize portable data only through public pgorm Value constructors."""

from datetime import date, datetime, time
from decimal import Decimal
import struct
from uuid import UUID

from . import wire


def kind(tag, p):
    if tag["kind"] == "enum":
        return p.TypeName(tag["name"], schema=tag["schema"])
    return tag["kind"]


def data_value(tag, data):
    name = tag["kind"]
    if name in wire.INTEGER_BITS:
        return int(data)
    if name in ("f32", "f64"):
        return struct.unpack(">f" if name == "f32" else ">d", bytes.fromhex(data))[0]
    if name in ("bytes", "mac_address"):
        return bytes(data)
    if name == "decimal":
        return Decimal(data)
    if name == "uuid":
        return UUID(data)
    if name == "date":
        return date.fromisoformat(data)
    if name == "time":
        return time.fromisoformat(data)
    if name.startswith("datetime"):
        return datetime.fromisoformat(wire.temporal_text(data))
    if name == "vector":
        return [struct.unpack(">f", bytes.fromhex(item))[0] for item in data]
    return data


def equivalent_snapshot(expected, actual):
    """Only alternate spellings of the same temporal value may compare equal."""
    if expected == actual:
        return True
    if expected["type"] != actual["type"] or expected["sql_null"] != actual["sql_null"]:
        return False
    name = expected["type"]["kind"]
    if name == "array" and not expected["sql_null"]:
        return len(expected["data"]) == len(actual["data"]) and all(
            equivalent_snapshot(a, b)
            for a, b in zip(expected["data"], actual["data"], strict=True)
        )
    if name.startswith("datetime") and not expected["sql_null"]:
        first = datetime.fromisoformat(wire.temporal_text(expected["data"]))
        second = datetime.fromisoformat(wire.temporal_text(actual["data"]))
        return first == second and first.utcoffset() == second.utcoffset()
    return False


# [spec:pgorm:req:generative.execution]
def materialize(snapshot, p):
    wire.validate(snapshot)
    tag = snapshot["type"]
    if tag["kind"] == "array":
        values = (
            None
            if snapshot["sql_null"]
            else [materialize(item, p) for item in snapshot["data"]]
        )
        result = p.Value.array(kind(tag["element"], p), values)
    elif snapshot["sql_null"]:
        result = p.Value.null(kind(tag, p))
    elif tag["kind"] == "json":
        result = p.Value.json(snapshot["data"])
    else:
        result = p.Value(data_value(tag, snapshot["data"]), kind(tag, p))
    if not equivalent_snapshot(snapshot, result.snapshot()):
        raise wire.FormatError("public Value conversion changed the portable input")
    return result
