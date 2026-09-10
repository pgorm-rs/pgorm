"""Validate the closed catalog's option types without executing instructions."""

import re

from . import baseline, wire


def identity(value):
    if not isinstance(value, str) or not re.fullmatch(r"[a-z][a-z0-9_]{0,47}", value):
        raise wire.FormatError(
            "artifact references must be bounded lowercase identifiers"
        )
    return value


def _integer(value, minimum):
    if type(value) is not int or not minimum <= value <= 2**63 - 1:
        raise wire.FormatError("integer option is outside its declared bounds")


def _strings(value, identifiers):
    if not isinstance(value, list) or len(value) > 256:
        raise wire.FormatError("string lists must be bounded arrays")
    for item in value:
        validate(item, "identifier" if identifiers else "string")


def validate(value, kind):
    if isinstance(kind, tuple):
        if not isinstance(value, str) or value not in kind:
            raise wire.FormatError("unknown option; expected one of " + ", ".join(kind))
    elif kind == "id":
        identity(value)
    elif kind == "identifier":
        baseline.identifier(value)
    elif kind in ("string", "character", "registration"):
        if not isinstance(value, str) or len(value.encode("utf-8")) > 65536:
            raise wire.FormatError("text option must be a bounded Unicode string")
        if kind == "character" and len(value) != 1:
            raise wire.FormatError("character option must have one Unicode scalar")
        if kind == "registration" and not re.fullmatch(
            r"[A-Za-z][A-Za-z0-9_.]{0,127}", value
        ):
            raise wire.FormatError("invalid compiled registration name")
    elif kind == "bool":
        if type(value) is not bool:
            raise wire.FormatError("boolean option requires an exact boolean")
    elif kind in ("integer", "nonnegative", "positive"):
        _integer(value, {"integer": -(2**63), "nonnegative": 0, "positive": 1}[kind])
    elif kind in ("strings", "identifiers"):
        _strings(value, kind == "identifiers")
    elif kind == "type":
        wire.type_tag(value)
    elif kind == "value":
        wire.validate(value)
    elif kind == "field_map":
        if not isinstance(value, dict) or not 1 <= len(value) <= 32:
            raise wire.FormatError(
                "model field mappings must be bounded nonempty objects"
            )
        for key, column in value.items():
            baseline.identifier(key)
            baseline.identifier(column)
        if len(set(value.values())) != len(value):
            raise wire.FormatError("model fields cannot ambiguously repeat a column")
    elif kind == "fixture_columns":
        fixture = baseline.default()
        fixture["tables"] = [
            {"schema": "fixture", "name": "validation", "columns": value, "rows": []}
        ]
        baseline.render(fixture)
    else:
        raise wire.FormatError("unknown option validator in instruction catalog")


def options(data, schema):
    optional = {
        key
        for key, kind in schema.items()
        if isinstance(kind, str) and kind.endswith("?")
    }
    wire.fields(data, set(schema) - optional, optional)
    for key, value in data.items():
        kind = schema[key]
        validate(value, kind.removesuffix("?") if isinstance(kind, str) else kind)


def input_ids(inputs):
    for value in inputs.values():
        if isinstance(value, list):
            yield from value
        else:
            yield value


def inputs(values, schema, types):
    optional = {key for key, kind in schema.items() if kind.endswith("?")}
    wire.fields(values, set(schema) - optional, optional)
    for key, value in values.items():
        signature = schema[key]
        variadic = signature.endswith("*")
        allowed = signature.rstrip("?*").split("|")
        if variadic:
            if not isinstance(value, list) or len(value) > 256:
                raise wire.FormatError(
                    "variadic inputs must be bounded reference lists"
                )
            references = value
        else:
            references = [value]
        for reference in references:
            identity(reference)
            if reference not in types:
                raise wire.FormatError(
                    "forward, missing or cyclic instruction reference: " + reference
                )
            if types[reference] not in allowed:
                raise wire.FormatError("instruction input has the wrong type: " + key)
