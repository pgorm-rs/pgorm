"""Versioned, random-access generation with no interpreter-dependent RNG state."""

from hashlib import sha256

from . import wire
from .corpus import Input
from .corpus_builtin import ENUM, STRINGS

VERSION = 1
KINDS = tuple(sorted(wire.SCALARS)) + ("enum", "array")


def entropy(seed, index):
    if any(type(v) is not int or not 0 <= v < 2**64 for v in (seed, index)):
        raise ValueError("corpus seed and index must be unsigned 64-bit integers")
    return sha256(
        b"pgorm-corpus-v1\0" + seed.to_bytes(8, "big") + index.to_bytes(8, "big")
    ).digest()


def _value(kind, raw):
    number = int.from_bytes(raw, "big")
    text = raw.hex()
    if kind in wire.INTEGER_BITS:
        bits, signed = wire.INTEGER_BITS[kind]
        return wire.scalar(
            kind, str(number % 2**bits - (2 ** (bits - 1) if signed else 0))
        )
    if kind in ("f32", "f64"):
        size, mask = (4, 0xFF7FFFFF) if kind == "f32" else (8, 0xFFEFFFFFFFFFFFFF)
        return wire.scalar(
            kind, (int.from_bytes(raw[:size], "big") & mask).to_bytes(size, "big").hex()
        )
    if kind == "enum":
        return wire.scalar(ENUM, ("calm", "O'Brien 雪")[number % 2])
    if kind == "array":
        tag = {"kind": "array", "element": {"kind": "text"}}
        return wire.scalar(
            tag, [wire.scalar("text", text), wire.scalar("text", None, sql_null=True)]
        )
    if kind == "decimal":
        scale, coefficient = number % 29, str(number % 2**96)
        coefficient = coefficient.zfill(scale + 1)
        value = (
            coefficient[:-scale] + "." + coefficient[-scale:] if scale else coefficient
        )
        return wire.scalar(kind, ("-" if raw[0] & 1 else "") + value)
    if kind.startswith("datetime") or kind in ("date", "time"):
        return wire.scalar(kind, _temporal(kind, raw))
    hostile = tuple(text for values in STRINGS.values() for text in values[:-1])
    data = {
        "bool": bool(raw[0] & 1),
        "text": hostile[number % len(hostile)] + text,
        "char": chr(0x20 + number % (0xD800 - 0x20)),
        "bytes": list(raw),
        "uuid": f"{text[:8]}-{text[8:12]}-{text[12:16]}-{text[16:20]}-{text[20:32]}",
        "json": {text[:8]: [number % 2**64, None, "'\\雪"]},
        "ipnetwork": f"{raw[0]}.{raw[1]}.{raw[2]}.{raw[3]}/{raw[4] % 33}",
        "mac_address": list(raw[:6]),
        "vector": ["3f" + text[:6], "bf" + text[6:12]],
    }[kind]
    return wire.scalar(kind, data)


def _temporal(kind, raw):
    day = f"{1900 + raw[0]:04d}-{1 + raw[1] % 12:02d}-{1 + raw[2] % 28:02d}"
    time = f"{raw[3] % 24:02d}:{raw[4] % 60:02d}:{raw[5] % 60:02d}"
    fraction = int.from_bytes(raw[6:9], "big") % 1000000
    if fraction:
        time += f".{fraction:06d}"
    if kind == "date":
        return day
    if kind == "time":
        return time
    value = day + " " + time
    if kind == "datetime_fixed":
        return value + f"+{raw[9] % 14:02d}:{(raw[10] % 4) * 15:02d}"
    return value + ("+00:00" if kind != "datetime" else "")


# [spec:pgorm:req:generative.corpus]
def sample(seed, index):
    raw = entropy(seed, index)
    return Input.create(
        f"random/{VERSION}/{seed}/{index}",
        "random",
        _value(KINDS[index % len(KINDS)], raw),
        {
            "kind": "random",
            "version": VERSION,
            "seed": seed,
            "index": index,
            "algorithm": "sha256-counter",
        },
    )
