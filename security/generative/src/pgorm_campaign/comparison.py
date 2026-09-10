"""Compare typed observations with explicit ordering and multiset semantics."""

from collections import Counter
from dataclasses import dataclass
from datetime import date, datetime, time
import json

from . import wire


class InvalidOracle(ValueError):
    """Missing or malformed oracle evidence cannot establish a verdict."""


@dataclass(frozen=True)
class Comparison:
    equal: bool
    reason: str


def value_key(value):
    wire.validate(value)
    tag = value["type"]
    kind = tag["kind"]
    data = value["data"]
    if not value["sql_null"]:
        if kind == "array":
            data = [value_key(item) for item in data]
        elif kind in ("date", "time"):
            parser = date if kind == "date" else time
            data = parser.fromisoformat(data).isoformat()
        elif kind.startswith("datetime"):
            parsed = datetime.fromisoformat(wire.temporal_text(data))
            data = parsed.isoformat(timespec="microseconds")
    return {"type": tag, "sql_null": value["sql_null"], "data": data}


def row_key(row):
    if not isinstance(row, dict):
        raise InvalidOracle("a row observation must be an object")
    kind = row.get("kind")
    if kind == "absent":
        wire.fields(row, {"kind"})
        return row
    if kind == "tuple":
        wire.fields(row, {"kind", "items"})
        if not isinstance(row["items"], list) or not row["items"]:
            raise InvalidOracle("source tuple must have at least one slot")
        return {"kind": "tuple", "items": [row_key(item) for item in row["items"]]}
    if kind != "record":
        raise InvalidOracle("unknown row observation kind")
    wire.fields(row, {"kind", "fields"}, {"entity", "postgres"})
    if not isinstance(row["fields"], list) or not row["fields"]:
        raise InvalidOracle("record observation has no fields")
    fields, names = [], set()
    for field in row["fields"]:
        wire.fields(field, {"name", "value"})
        if not isinstance(field["name"], str) or field["name"] in names:
            raise InvalidOracle("record output names must be unique strings")
        names.add(field["name"])
        fields.append({"name": field["name"], "value": value_key(field["value"])})
    result = {**row, "fields": fields}
    if "entity" in row and not isinstance(row["entity"], str):
        raise InvalidOracle("entity observation needs its registered identity")
    return result


def encoded(value):
    return json.dumps(
        value,
        sort_keys=True,
        ensure_ascii=False,
        allow_nan=False,
        separators=(",", ":"),
    )


# [spec:pgorm:req:generative.comparison]
def rows(actual, expected, *, ordered):
    if (
        type(ordered) is not bool
        or not isinstance(actual, list)
        or not isinstance(expected, list)
    ):
        raise InvalidOracle(
            "row comparison needs two explicit lists and an ordering policy"
        )
    first = [encoded(row_key(row)) for row in actual]
    second = [encoded(row_key(row)) for row in expected]
    if ordered:
        equal = first == second
    else:
        equal = Counter(first) == Counter(second)
    return Comparison(
        equal,
        "equal"
        if equal
        else "ordered rows differ"
        if ordered
        else "row multisets differ",
    )


def exact_error(actual, expected):
    wire.fields(expected, {"class", "cause"})
    if not isinstance(expected["cause"], str) or not expected["cause"]:
        raise InvalidOracle("expected rejection requires a specific cause")
    if actual.get("kind") != "error" or actual.get("class") != expected["class"]:
        return Comparison(False, "error category differs")
    cause = expected["cause"]
    if cause.startswith("sqlstate:"):
        code = cause.removeprefix("sqlstate:")
        if len(code) != 5 or not code.isascii() or not code.isalnum():
            raise InvalidOracle("expected SQLSTATE must be exactly five characters")
        equal = actual.get("sqlstate") == code
    else:
        equal = actual.get("cause") == cause
    return Comparison(equal, "equal" if equal else "error cause differs")


def streamed(actual, expected, ordered):
    check = expected["stream_check"]
    wire.fields(check, {"take", "cancel"})
    take, cancel = check["take"], check["cancel"]
    if type(take) is not int or take < 0 or type(cancel) is not bool:
        raise InvalidOracle("malformed reference stream invariant")
    available = expected["rows"]
    received = actual["rows"]
    if len(received) != min(take, len(available)):
        return Comparison(False, "stream yielded the wrong row count")
    if ordered:
        result = rows(received, available[:take], ordered=True)
    else:
        actual_bag = Counter(encoded(row_key(row)) for row in received)
        permitted_bag = Counter(encoded(row_key(row)) for row in available)
        equal = all(count <= permitted_bag[key] for key, count in actual_bag.items())
        result = Comparison(
            equal,
            "equal" if equal else "stream yielded rows outside the reference multiset",
        )
    if not result.equal:
        return result
    state = actual.get("stream")
    if (
        not isinstance(state, dict)
        or set(state) != {"complete", "cancelled", "closed"}
        or any(type(value) is not bool for value in state.values())
    ):
        raise InvalidOracle("malformed subject stream lifecycle observation")
    if len(available) < take:
        allowed = {(True, False)}
    elif not cancel:
        allowed = {(False, False)}
    elif len(available) == take:
        allowed = {(True, False), (False, True)}
    else:
        # A cancellation can race a successfully yielded, unobserved next row.
        allowed = {(False, False), (False, True)}
    equal = state["closed"] and (state["complete"], state["cancelled"]) in allowed
    return Comparison(
        equal, "equal" if equal else "stream lifecycle violates the declared operation"
    )


def observation(actual, expected, *, ordered=False):
    if not isinstance(actual, dict) or not isinstance(expected, dict):
        raise InvalidOracle("effect comparison requires observation objects")
    if actual.get("kind") != expected.get("kind"):
        return Comparison(False, "observation categories differ")
    kind = expected.get("kind")
    if kind == "rows":
        if "stream_check" in expected:
            return streamed(actual, expected, ordered)
        result = rows(actual["rows"], expected["rows"], ordered=ordered)
        if not result.equal:
            return result
        equal = actual.get("stream") == expected.get("stream")
    elif kind == "count":
        if (
            type(actual.get("value")) is not int
            or type(expected.get("value")) is not int
        ):
            raise InvalidOracle("affected-row count must be an integer")
        equal = actual["value"] == expected["value"]
    elif kind in ("transaction", "unit"):
        equal = actual == expected
    else:
        raise InvalidOracle(
            "no independent comparison for observation kind: " + str(kind)
        )
    return Comparison(equal, "equal" if equal else "effect observations differ")
