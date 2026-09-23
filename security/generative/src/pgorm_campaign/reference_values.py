"""Independent parameter adaptation; no pgorm conversions or rendered literals."""

from dataclasses import dataclass
import json
import math
import struct

from . import wire
from .comparison import InvalidOracle


@dataclass(frozen=True)
class Float32:
    data: bytes


@dataclass(frozen=True)
class Float64:
    data: bytes


def quote(name):
    if not isinstance(name, str) or not name or "\0" in name:
        raise InvalidOracle("reference identifier must be nonempty NUL-free text")
    if len(name.encode("utf-8")) > 63:
        raise InvalidOracle("reference identifier exceeds PostgreSQL's name limit")
    return '"' + name.replace('"', '""') + '"'


def qualified(name, schema=None):
    return (quote(schema) + "." if schema is not None else "") + quote(name)


TYPES = {
    "bool": "boolean",
    "i8": "smallint",
    "i16": "smallint",
    "i32": "integer",
    "i64": "bigint",
    "u32": "oid",
    # PostgreSQL has no unsigned 64-bit type: pgorm writes a u64 as int8 and
    # refuses one past i64::MAX before sending it (`reference_sql.refuse`).
    "u64": "bigint",
    "f32": "real",
    "f64": "double precision",
    "text": "text",
    "char": "text",
    "bytes": "bytea",
    "decimal": "numeric",
    "uuid": "uuid",
    "json": "jsonb",
    "date": "date",
    "time": "time without time zone",
    "datetime": "timestamp without time zone",
    "datetime_utc": "timestamp with time zone",
    "ipnetwork": "inet",
    "mac_address": "macaddr",
    # pgvector's own type, named as pgorm names it: unqualified, resolved on
    # the search path. The pinned image does not install it, so any statement
    # naming it is refused by the server before a value is examined.
    "vector": "vector",
}


def sql_type(tag):
    kind = tag["kind"]
    if kind == "enum":
        return qualified(tag["name"], tag["schema"])
    if kind == "array":
        element = tag["element"]
        return sql_type(element) + "[]"
    if kind not in TYPES:
        raise InvalidOracle("no installed PostgreSQL reference type for " + kind)
    return TYPES[kind]


def argument(value):
    wire.validate(value)
    if value["sql_null"]:
        return None
    kind, data = value["type"]["kind"], value["data"]
    if kind == "f32":
        return Float32(bytes.fromhex(data))
    if kind == "f64":
        return Float64(bytes.fromhex(data))
    if kind == "bytes":
        return bytes(data)
    if kind == "json":
        return json.dumps(data, ensure_ascii=False, allow_nan=False)
    if kind == "mac_address":
        return ":".join(f"{byte:02x}" for byte in data)
    if kind.startswith("datetime"):
        return wire.temporal_text(data)
    if kind == "vector":
        return vector_text(value)
    if kind == "array":
        raise InvalidOracle("reference arrays require explicit element parameters")
    return data


def vector_text(value):
    """pgvector's text input for a vector: each f32 at its shortest exact spelling."""
    items = []
    for bits in value["data"]:
        number = struct.unpack(">f", bytes.fromhex(bits))[0]
        if not math.isfinite(number):
            raise InvalidOracle("pgvector admits only finite elements")
        spellings = (format(number, f".{digits}g") for digits in range(1, 10))
        items.append(
            next(
                text
                for text in spellings
                if struct.pack(">f", float(text)).hex() == bits
            )
        )
    return "[" + ",".join(items) + "]"


def install(connection):
    from psycopg.adapt import Dumper
    from psycopg.pq import Format

    class Float32Dumper(Dumper):
        format = Format.BINARY
        oid = 700

        def dump(self, value):
            return value.data

    class Float64Dumper(Float32Dumper):
        oid = 701

    connection.adapters.register_dumper(Float32, Float32Dumper)
    connection.adapters.register_dumper(Float64, Float64Dumper)
