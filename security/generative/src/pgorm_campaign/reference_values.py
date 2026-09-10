"""Independent parameter adaptation; no pgorm conversions or rendered literals."""

from dataclasses import dataclass
import json

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
    "datetime_fixed": "timestamp with time zone",
    "datetime_local": "timestamp with time zone",
    "ipnetwork": "inet",
    "mac_address": "macaddr",
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
    if kind == "array":
        raise InvalidOracle("reference arrays require explicit element parameters")
    return data


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
