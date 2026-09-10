"""Lossless tagged values for portable programs, independent of native loading."""

from datetime import date, datetime, time
from decimal import Decimal, InvalidOperation
import ipaddress
import re
import uuid

from .baseline import identifier

INTEGER_BITS = {
    "i8": (8, True),
    "i16": (16, True),
    "i32": (32, True),
    "i64": (64, True),
    "u32": (32, False),
    "u64": (64, False),
}
SCALARS = frozenset(INTEGER_BITS) | {
    "bool",
    "f32",
    "f64",
    "text",
    "char",
    "bytes",
    "decimal",
    "uuid",
    "json",
    "date",
    "time",
    "datetime",
    "datetime_utc",
    "datetime_fixed",
    "datetime_local",
    "ipnetwork",
    "mac_address",
    "vector",
}


class FormatError(ValueError):
    """A portable artifact is malformed or outside its declared limits."""


def fields(value, required, optional=()):
    if not isinstance(value, dict) or not set(required) <= value.keys() <= set(
        required
    ) | set(optional):
        raise FormatError(
            "unexpected object fields; required: " + ", ".join(sorted(required))
        )


def type_tag(tag):
    if not isinstance(tag, dict) or not isinstance(tag.get("kind"), str):
        raise FormatError("a type tag requires a kind")
    kind = tag["kind"]
    if kind in SCALARS:
        fields(tag, {"kind"})
    elif kind == "enum":
        fields(tag, {"kind", "name", "schema"})
        identifier(tag["name"])
        if tag["schema"] is not None:
            identifier(tag["schema"])
    elif kind == "array":
        fields(tag, {"kind", "element"})
        type_tag(tag["element"])
        if tag["element"]["kind"] == "array":
            raise FormatError("nested arrays are unsupported")
    else:
        raise FormatError("unknown portable value kind")
    return tag


def _integer(kind, data):
    if not isinstance(data, str) or not re.fullmatch(r"0|-?[1-9][0-9]{0,19}", data):
        raise FormatError("integer payload must be canonical decimal text")
    bits, signed = INTEGER_BITS[kind]
    low = -(2 ** (bits - 1)) if signed else 0
    high = 2 ** (bits - int(signed)) - 1
    if not low <= int(data) <= high:
        raise FormatError("integer payload is outside its declared Rust variant")


def _bits(data, size):
    if not isinstance(data, str) or not re.fullmatch(
        "[0-9a-f]{" + str(size) + "}", data
    ):
        raise FormatError("float payload must be an exact lowercase IEEE bit pattern")


def _bytes(data, length=None):
    if (
        not isinstance(data, list)
        or len(data) > 65536
        or any(type(v) is not int or not 0 <= v <= 255 for v in data)
    ):
        raise FormatError("byte payload must be a bounded list of bytes")
    if length is not None and len(data) != length:
        raise FormatError("byte payload has the wrong length")


def _decimal(data):
    if not isinstance(data, str) or not re.fullmatch(
        r"-?(0|[1-9][0-9]*)(\.[0-9]+)?", data
    ):
        raise FormatError("decimal payload must preserve its coefficient and scale")
    try:
        number = Decimal(data)
    except InvalidOperation as error:
        raise FormatError("invalid decimal payload") from error
    _, digits, exponent = number.as_tuple()
    coefficient = int("".join(map(str, digits)))
    if not -28 <= exponent <= 0 or coefficient >= 2**96:
        raise FormatError("decimal payload exceeds the native coefficient or scale")


def temporal_text(data):
    """Accept native Chrono display text while retaining the original artifact."""
    value = data.removesuffix(" UTC") + ("+00:00" if data.endswith(" UTC") else "")
    value = re.sub(r" ([+-][0-9]{2}:[0-9]{2})$", r"\1", value)
    value = value.replace("T", " ").removesuffix("Z") + (
        "+00:00" if value.endswith("Z") else ""
    )
    match = re.search(r"\.(\d+)", value)
    if match:
        digits = match[1]
        if len(digits) > 6:
            raise FormatError("temporal payload exceeds Python's microsecond precision")
        fraction = "." + digits.ljust(6, "0") if int(digits) else ""
        value = value[: match.start()] + fraction + value[match.end() :]
    return value


def _temporal(kind, data):
    if not isinstance(data, str):
        raise FormatError("temporal payload must be exact ISO text")
    normalized = temporal_text(data)
    try:
        if kind == "date":
            value = date.fromisoformat(normalized)
            canonical = value.isoformat()
        elif kind == "time":
            value = time.fromisoformat(normalized)
            if value.tzinfo is not None:
                raise FormatError("time payload cannot have an offset")
            canonical = value.isoformat()
        else:
            value = datetime.fromisoformat(normalized)
            if (value.utcoffset() is None) != (kind == "datetime"):
                raise FormatError("datetime kind and timezone do not agree")
            if kind == "datetime_utc" and value.utcoffset().total_seconds() != 0:
                raise FormatError("UTC datetime requires a zero offset")
            if value.utcoffset() is not None and value.utcoffset().microseconds:
                raise FormatError("subsecond timezone offsets are unsupported")
            canonical = value.isoformat(sep=" ")
        # ISO parsers accept and truncate extra precision; preserve it or reject it.
        if normalized != canonical:
            raise FormatError("temporal text is noncanonical or loses precision")
    except ValueError as error:
        raise FormatError("invalid or unrepresentable temporal payload") from error


def _json(value, depth=0):
    if depth > 64:
        raise FormatError("JSON payload exceeds its nesting budget")
    if value is None or type(value) in (bool, str):
        return
    if type(value) is int and -(2**63) <= value <= 2**64 - 1:
        return
    if type(value) is float:
        import math

        if math.isfinite(value):
            return
    if isinstance(value, list) and len(value) <= 4096:
        for item in value:
            _json(item, depth + 1)
        return
    if (
        isinstance(value, dict)
        and len(value) <= 4096
        and all(type(k) is str for k in value)
    ):
        for item in value.values():
            _json(item, depth + 1)
        return
    raise FormatError("JSON payload cannot preserve the given value")


def _scalar(kind, data):
    if kind in INTEGER_BITS:
        _integer(kind, data)
    elif kind in ("f32", "f64"):
        _bits(data, 8 if kind == "f32" else 16)
    elif kind == "bool":
        if type(data) is not bool:
            raise FormatError("bool payload requires an exact boolean")
    elif kind in ("text", "char", "enum"):
        if not isinstance(data, str) or len(data.encode("utf-8")) > 65536:
            raise FormatError("text payload must be a bounded Unicode string")
        if kind == "char" and len(data) != 1:
            raise FormatError("character payload must have one Unicode scalar")
    elif kind in ("bytes", "mac_address"):
        _bytes(data, 6 if kind == "mac_address" else None)
    elif kind == "decimal":
        _decimal(data)
    elif kind == "uuid":
        if not isinstance(data, str) or str(uuid.UUID(data)) != data:
            raise FormatError("UUID payload must be canonical text")
    elif kind == "ipnetwork":
        if not isinstance(data, str) or str(ipaddress.ip_interface(data)) != data:
            raise FormatError(
                "IP network payload must preserve canonical host/prefix text"
            )
    elif kind in ("date", "time") or kind.startswith("datetime"):
        _temporal(kind, data)
    elif kind == "json":
        _json(data)
    elif kind == "vector":
        if not isinstance(data, list) or len(data) > 4096:
            raise FormatError("vector payload must be a bounded list")
        for item in data:
            _bits(item, 8)


# [spec:pgorm:req:generative.format]
def validate(value):
    """Validate tagged data without importing pgorm or constructing native objects."""
    fields(value, {"version", "type", "sql_null", "data"})
    if type(value["version"]) is not int or value["version"] != 1:
        raise FormatError("unsupported value format version")
    tag = type_tag(value["type"])
    if type(value["sql_null"]) is not bool:
        raise FormatError("SQL NULL flag must be a boolean")
    if value["sql_null"]:
        if value["data"] is not None:
            raise FormatError("SQL NULL cannot contain a data payload")
    elif tag["kind"] == "array":
        if not isinstance(value["data"], list) or len(value["data"]) > 4096:
            raise FormatError("array payload must be a bounded list of tagged elements")
        for item in value["data"]:
            validate(item)
            if item["type"] != tag["element"]:
                raise FormatError("array element has a different type identity")
    else:
        _scalar(tag["kind"], value["data"])
    return value


def scalar(kind, data, *, sql_null=False):
    tag = {"kind": kind} if isinstance(kind, str) else kind
    return validate({"version": 1, "type": tag, "sql_null": sql_null, "data": data})
