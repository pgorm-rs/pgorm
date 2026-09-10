"""Model literal values and PostgreSQL type inference using bound parameters.

This module never escapes or emits user data into SQL. Unknown string/NULL
parameters let PostgreSQL infer the type from the consuming expression. Numeric
parameters carry the type implied by the literal's documented decimal spelling.
"""

from datetime import datetime, time
from decimal import Decimal
import json
import math
import struct

from . import wire
from .comparison import InvalidOracle
from .reference_values import argument, sql_type


def number_type(text):
    if "." in text:
        return "numeric"
    value = int(text)
    if -(2**31) <= value < 2**31:
        return "integer"
    if -(2**63) <= value < 2**63:
        return "bigint"
    return "numeric"


def float_text(value, pipeline):
    kind, data = value["type"]["kind"], bytes.fromhex(value["data"])
    number = struct.unpack("!f" if kind == "f32" else "!d", data)[0]
    if not math.isfinite(number):
        raise InvalidOracle("non-finite inline floats need a specific rejection oracle")
    candidate = repr(number)
    if kind == "f32":
        for precision in range(1, 10):
            candidate = format(number, f".{precision}g")
            try:
                if struct.pack("!f", float(candidate)) == data:
                    break
            except OverflowError:
                continue
    text = format(Decimal(candidate), "f")
    if "." in text:
        text = text.rstrip("0").rstrip(".")
    return text + ".0" if pipeline and "." not in text else text


def textual(value):
    kind, data = value["type"]["kind"], value["data"]
    if kind == "bytes":
        return "\\x" + bytes(data).hex().upper()
    if kind == "json":
        return json.dumps(
            data, ensure_ascii=False, separators=(",", ":"), sort_keys=True
        )
    if kind == "time":
        # sql.render.value-literals+2 explicitly specifies whole seconds here.
        return time.fromisoformat(data).replace(microsecond=0).isoformat()
    if kind.startswith("datetime"):
        value = datetime.fromisoformat(wire.temporal_text(data)).replace(microsecond=0)
        if kind == "datetime":
            return value.isoformat(sep=" ")
        text = value.strftime("%Y-%m-%d %H:%M:%S %z")
        return text[:-2] + ":" + text[-2:]
    if kind == "vector":
        raise InvalidOracle("vector literal needs an installed pgvector oracle")
    return argument(value)


def literal(value, *, pipeline=False):
    from .reference_sql import SQL, Parameter, bound, join

    tag, kind = value["type"], value["type"]["kind"]
    if pipeline and (
        value["sql_null"] or kind not in ("bool", "i32", "i64", "f64", "text")
    ):
        raise InvalidOracle(
            "this typed value is outside the public pipeline literal surface"
        )
    if value["sql_null"]:
        return SQL((Parameter(wire.scalar("text", None, sql_null=True)),))
    if kind == "bool":
        return bound(value)
    if kind == "array":
        result = "ARRAY[" + join([literal(item) for item in value["data"]]) + "]"
        if not value["data"] and tag["element"]["kind"] not in ("json", "vector"):
            element = tag["element"]
            carrier = (
                "bigint"
                if element["kind"] in ("u32", "u64")
                else "text"
                if element["kind"] == "enum"
                else sql_type(element)
            )
            result += "::" + carrier + "[]"
        return result
    if kind in wire.INTEGER_BITS or kind in ("decimal", "f32", "f64"):
        text = float_text(value, pipeline) if kind in ("f32", "f64") else value["data"]
        return SQL((Parameter(wire.scalar("text", text)), "::" + number_type(text)))
    if kind == "enum":
        # The public Value enum tag ascribes the constant's PostgreSQL type.
        return bound(value)
    return SQL((Parameter(wire.scalar("text", textual(value))),))
