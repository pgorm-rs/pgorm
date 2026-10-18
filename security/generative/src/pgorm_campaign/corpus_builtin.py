"""Repository-owned hostile inputs and lossless native type boundaries."""

from . import wire
from .corpus import VERSION, Input

ENUM = {"kind": "enum", "schema": "fixture", "name": 'State" 雪'}
STRINGS = {
    "quotes": ("'", '"', "''", '""', "O'Reilly", "' OR TRUE --"),
    "comments": ("--", "--\n", "/* */", "/* outer /* inner */ end */", "#"),
    "separators": (";", ";SELECT 1;--", "a;b", "\r\n\t"),
    "dollar-strings": ("$$", "$tag$'--;$tag$", "$1", "$01", "$1$"),
    "backslashes": ("\\", "\\'", "\\\\", r"\x00", r"\u0027"),
    "unicode": ("雪", "💩", "é", "e\u0301", "\u202e'", "\u200b", "\uff07"),
    "wildcards": ("%", "_", r"\%\_", "%_%", "a%b_c"),
    "text-boundaries": (
        "",
        "\0",
        "a\0b",
        "a" * 63,
        "a" * 64,
        "a" * 65,
        "雪" * 21,
        "雪" * 21 + "a",
        "雪" * 21 + "aa",
        "a" * 65536,
    ),
}
FLOATS = {
    "f32": (
        "00000000",
        "80000000",
        "00000001",
        "80000001",
        "007fffff",
        "00800000",
        "7f7fffff",
        "ff7fffff",
        "7f800000",
        "ff800000",
        "7fc00000",
        "7fc00001",
        "ffc00001",
    ),
    "f64": (
        "0000000000000000",
        "8000000000000000",
        "0000000000000001",
        "8000000000000001",
        "000fffffffffffff",
        "0010000000000000",
        "7fefffffffffffff",
        "ffefffffffffffff",
        "7ff0000000000000",
        "fff0000000000000",
        "7ff8000000000000",
        "7ff8000000000001",
        "fff8000000000001",
    ),
}


def _scalars():
    for family, values in STRINGS.items():
        for value in values:
            yield family, wire.scalar("text", value)
    for kind, (bits, signed) in wire.INTEGER_BITS.items():
        low, high = (-(2 ** (bits - 1)) if signed else 0), 2 ** (bits - int(signed)) - 1
        for value in sorted(
            {low, low + 1, 0, 1, high - 1, high} | ({-1} if signed else set())
        ):
            yield "integer-boundaries", wire.scalar(kind, str(value))
    for kind, values in FLOATS.items():
        for value in values:
            yield "float-boundaries", wire.scalar(kind, value)
    for kind, values in _other().items():
        for value in values:
            yield "type-boundaries", wire.scalar(kind, value)


def _other():
    return {
        "bool": (False, True),
        "char": ("\0", "'", "雪", "💩"),
        "bytes": ([], [0], [255], list(range(256))),
        "decimal": (
            "0",
            "-0.00",
            "1.00",
            "0.0000000000000000000000000001",
            "79228162514264337593543950335",
            "-79228162514264337593543950335",
        ),
        "uuid": (
            "00000000-0000-0000-0000-000000000000",
            "ffffffff-ffff-ffff-ffff-ffffffffffff",
        ),
        "json": (
            None,
            [],
            {},
            {"'": [None, True, "\0", "雪", 2**64 - 1]},
            "' OR TRUE --",
        ),
        "date": ("0001-01-01", "1970-01-01", "2000-02-29", "9999-12-31"),
        "time": ("00:00:00", "00:00:00.000001", "23:59:59.999999"),
        "datetime": (
            "0001-01-01 00:00:00",
            "2000-02-29 12:34:56.123456",
            "9999-12-31 23:59:59.999999",
        ),
        "datetime_utc": (
            "1970-01-01 00:00:00+00:00",
            "2000-02-29 12:34:56.123456+00:00",
        ),
        "ipnetwork": (
            "0.0.0.0/0",
            "255.255.255.255/32",
            "::/0",
            "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff/128",
        ),
        "mac_address": ([0] * 6, [255] * 6),
        "vector": ([], ["00000000", "80000000", "3f800000"], ["7fc00000"]),
        "interval": _intervals(),
    }


def _intervals():
    """Signs mixed across fields, single microseconds, each field's limits and
    both infinities. `1 mon` and `30 days`, and `1 day` and `24 hours`, are the
    pairs PostgreSQL's `=` equates while the values stay distinct; the triples a
    microsecond short of each infinity are finite.
    """
    i32, i64 = 2**31, 2**63
    return (
        wire.interval(0, 0, 0),
        wire.interval(1, 0, 0),
        wire.interval(0, 30, 0),
        wire.interval(0, 1, 0),
        wire.interval(0, 0, 86_400_000_000),
        wire.interval(1, -2, 0),
        wire.interval(1, -2, 3_000_004),
        wire.interval(-1, 2, -3_000_004),
        wire.interval(0, 0, 1),
        wire.interval(0, 0, -1),
        wire.interval(i32 - 1, 0, 0),
        wire.interval(-i32, 0, 0),
        wire.interval(0, i32 - 1, 0),
        wire.interval(0, -i32, 0),
        wire.interval(0, 0, i64 - 1),
        wire.interval(0, 0, -i64),
        wire.interval(i32 - 1, i32 - 1, i64 - 2),
        wire.interval(-i32, -i32, -i64 + 1),
        wire.INTERVAL_INFINITY,
        wire.INTERVAL_NEGATIVE_INFINITY,
    )


# [spec:pgorm:req:generative.corpus]
def builtin():
    """Type-valid data; acceptance or rejection still depends on the API context."""
    entries = list(_scalars())
    entries.extend(
        ("enum", wire.scalar(ENUM, label)) for label in ("calm", "O'Brien 雪")
    )
    tags = [{"kind": kind} for kind in sorted(wire.SCALARS)] + [ENUM]
    for tag in tags:
        entries.append(("sql-null", wire.scalar(tag, None, sql_null=True)))
    representatives = {}
    for _, value in entries:
        if not value["sql_null"]:
            representatives.setdefault(str(value["type"]), value)
    for value in representatives.values():
        tag = {"kind": "array", "element": value["type"]}
        null = wire.scalar(value["type"], None, sql_null=True)
        entries.extend(
            ("arrays", wire.scalar(tag, items))
            for items in ([], [value], [null], [value, null, value])
        )
        entries.append(("sql-null-array", wire.scalar(tag, None, sql_null=True)))
    return tuple(
        Input.create(
            f"builtin/{index}", family, value, {"kind": "builtin", "version": VERSION}
        )
        for index, (family, value) in enumerate(entries)
    )
