"""Read PostgreSQL binary observations without the subject's value decoder."""

import json
import struct

from . import wire
from .comparison import InvalidOracle

KINDS = {
    "bool": "bool",
    "char": "i8",
    "int2": "i16",
    "int4": "i32",
    "int8": "i64",
    "oid": "u32",
    "float4": "f32",
    "float8": "f64",
    "text": "text",
    "varchar": "text",
    "bpchar": "text",
    "name": "text",
    "bytea": "bytes",
    "numeric": "decimal",
    "uuid": "uuid",
    "json": "json",
    "jsonb": "json",
    "date": "date",
    "time": "time",
    "timestamp": "datetime",
    "timestamptz": "datetime_utc",
    "inet": "ipnetwork",
    "cidr": "ipnetwork",
    "macaddr": "mac_address",
}

CATALOG = """
SELECT t.oid, n.nspname, t.typname, t.typtype, t.typelem, t.typcategory
FROM pg_catalog.pg_type t
JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
"""


class Codec:
    def __init__(self, connection):
        self.connection = connection
        self.types = {}

    async def refresh(self):
        async with self.connection.cursor() as cursor:
            await cursor.execute(CATALOG)
            self.types = {row[0]: row[1:] for row in await cursor.fetchall()}

    def tag(self, oid):
        schema, name, category, element, family = self.types[oid]
        if family == "A" and element:
            return {"kind": "array", "element": self.tag(element)}
        if category == "e":
            return {"kind": "enum", "schema": schema, "name": name}
        if schema != "pg_catalog" or name not in KINDS:
            raise InvalidOracle(
                "unsupported reference result type: " + schema + "." + name
            )
        return {"kind": KINDS[name]}

    def array(self, raw, tag, element):
        if len(raw) < 12:
            raise InvalidOracle("truncated PostgreSQL array header")
        dimensions, has_null, actual_element = struct.unpack("!iiI", raw[:12])
        if (
            dimensions not in (0, 1)
            or has_null not in (0, 1)
            or actual_element != element
        ):
            raise InvalidOracle("unrepresentable PostgreSQL array shape or type")
        if dimensions == 0:
            if len(raw) != 12:
                raise InvalidOracle("empty array has trailing data")
            return []
        if len(raw) < 20:
            raise InvalidOracle("truncated array dimensions")
        length, lower = struct.unpack("!ii", raw[12:20])
        if lower != 1 or not 0 <= length <= 65536:
            raise InvalidOracle("array bounds exceed the portable representation")
        result, offset = [], 20
        for _ in range(length):
            if offset + 4 > len(raw):
                raise InvalidOracle("truncated array element length")
            size = struct.unpack("!i", raw[offset : offset + 4])[0]
            offset += 4
            if size < -1 or offset + max(0, size) > len(raw):
                raise InvalidOracle("truncated array element")
            data = None if size == -1 else raw[offset : offset + size]
            result.append(self.value(element, data))
            offset += max(0, size)
        if offset != len(raw) or any(v["type"] != tag["element"] for v in result):
            raise InvalidOracle("array data or element type mismatch")
        return result

    def scalar(self, oid, raw, kind):
        if kind in ("f32", "f64"):
            if len(raw) != (4 if kind == "f32" else 8):
                raise InvalidOracle("wrong binary float width")
            return raw.hex()
        if kind in wire.INTEGER_BITS:
            return str(int.from_bytes(raw, "big", signed=kind != "u32"))
        if kind in ("bytes", "mac_address"):
            return list(raw)
        if kind in ("text", "enum"):
            return raw.decode("utf-8")
        if kind == "json":
            if self.types[oid][1] == "jsonb":
                if raw[:1] != b"\x01":
                    raise InvalidOracle("unsupported JSONB binary version")
                raw = raw[1:]
            return json.loads(raw)
        loader = self.connection.adapters.get_loader(oid, 1)
        if loader is None:
            raise InvalidOracle("independent driver has no binary decoder")
        value = loader(oid, self.connection).load(raw)
        if kind == "bool":
            return value
        if kind == "decimal":
            # Decimal.__str__ may choose exponent notation at small scales;
            # the portable representation preserves the fixed coefficient/scale.
            return format(value, "f")
        if kind in ("date", "time") or kind.startswith("datetime"):
            return value.isoformat()
        if kind == "ipnetwork":
            text = str(value)
            return text if "/" in text else text + ("/128" if ":" in text else "/32")
        return str(value)

    # [spec:pgorm:req:generative.comparison]
    def value(self, oid, raw):
        tag = self.tag(oid)
        data = None
        if raw is not None:
            raw = bytes(raw)
            data = (
                self.array(raw, tag, self.types[oid][3])
                if tag["kind"] == "array"
                else self.scalar(oid, raw, tag["kind"])
            )
        return wire.validate(
            {"version": 1, "type": tag, "sql_null": raw is None, "data": data}
        )

    async def records(self, result):
        if any(
            result.ftype(index) not in self.types for index in range(result.nfields)
        ):
            await self.refresh()
        names = [result.fname(index).decode("utf-8") for index in range(result.nfields)]
        if len(set(names)) != len(names):
            raise InvalidOracle("independent result has duplicate output names")
        postgres = [
            {
                "name": name,
                "schema": self.types[result.ftype(i)][0],
                "type": self.types[result.ftype(i)][1],
            }
            for i, name in enumerate(names)
        ]
        return [
            {
                "kind": "record",
                "fields": [
                    {
                        "name": name,
                        "value": self.value(result.ftype(i), result.get_value(row, i)),
                    }
                    for i, name in enumerate(names)
                ],
                "postgres": postgres,
            }
            for row in range(result.ntuples)
        ]
