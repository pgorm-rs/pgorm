"""Semantic probes independent of the executor's dispatch smoke cases."""

from pgorm_campaign import wire
from pgorm_campaign.program import Program

from execution_cases import Case


def three_valued(mode):
    c = Case()
    table = c.node("table", data={"schema": "fixture", "name": "accounts"})
    score = c.node("expr.column", {"table": table}, {"name": "score"})
    identity = c.node("expr.column", {"table": table}, {"name": "id"})
    null = c.node("value", data={"value": wire.scalar("i32", None, sql_null=True)})
    ten = c.value("i32", 10)
    predicate = c.node(
        "expr.membership",
        {"value": score, "items": [ten, null] if mode == "null-membership" else []},
        {"negated": True},
    )
    if mode.startswith("empty-condition"):
        predicate = c.node(
            "condition",
            {"items": [predicate, c.node("condition", {"items": []}, {"mode": "any"})]},
            {"mode": "all"},
        )
    query = c.node("select", {"columns": [identity]})
    query = c.node("select.from", {"query": query, "table": table})
    query = c.node("select.filter", {"query": query, "predicate": predicate})
    # The unused NULL/ten nodes are omitted when testing empty operands.
    if mode != "null-membership":
        c.nodes = [node for node in c.nodes if node["id"] not in (null, ten)]
    c.fetch(query)
    return c.program()


def rejection():
    c = Case()
    query = c.node(
        "raw.template",
        {"parameters": [c.value("i32", 1), c.value("i32", 0)]},
        {"text": "SELECT $1::int4 / $2::int4 AS division"},
    )
    c.fetch(query)
    data = c.program().data()
    data["observations"][0] = {
        "step": "s0",
        "oracle": "exact-error",
        "error": {"class": "DatabaseError", "cause": "sqlstate:22012"},
    }
    return Program.from_dict(data)


def values(kind, data, postgres, schema="pg_catalog"):
    c, columns = Case(), []
    scalar = wire.scalar(kind, data)
    tag = scalar["type"]
    array = {"kind": "array", "element": tag}
    samples = [
        scalar,
        wire.scalar(tag, None, sql_null=True),
        wire.scalar(array, [scalar, wire.scalar(tag, None, sql_null=True)]),
        wire.scalar(array, []),
        wire.scalar(array, None, sql_null=True),
    ]
    for index, sample in enumerate(samples):
        value = c.node("value", data={"value": sample})
        expression = c.node("expr.value", {"value": value}, {"mode": "bound"})
        if tag["kind"] == "i8":
            expression = c.node(
                "expr.cast",
                {"value": expression},
                {
                    "name": "int4",
                    "schema": "pg_catalog",
                    "array": sample["type"]["kind"] == "array",
                },
            )
        expression = c.node(
            "expr.cast",
            {"value": expression},
            {
                "name": postgres,
                "schema": schema,
                "array": sample["type"]["kind"] == "array",
            },
        )
        columns.append(
            c.node("expr.alias", {"value": expression}, {"name": "v" + str(index)})
        )
    query = c.node("select", {"columns": columns})
    c.fetch(query)
    return c.program()


def type_cases(*, finite=False):
    samples = [
        ("bool", True, "bool"),
        ("i8", "-128", "char"),
        ("i16", "-32768", "int2"),
        ("i32", "-2147483648", "int4"),
        ("i64", "-9223372036854775808", "int8"),
        ("u32", "4294967295", "oid"),
        ("f32", "80000000", "float4"),
        ("f64", "3ff4000000000000" if finite else "7ff0000000000000", "float8"),
        ("text", "O'Brien 雪; -- %_\\", "text"),
        ("char", "雪", "text"),
        ("bytes", [0, 255, 39], "bytea"),
        ("json", None, "jsonb"),
        ("decimal", "-0.00100", "numeric"),
        ("uuid", "12345678-1234-5678-1234-567812345678", "uuid"),
        ("date", "2024-01-02", "date"),
        ("time", "03:04:05.123456", "time"),
        ("datetime", "2024-01-02 03:04:05.123456", "timestamp"),
        ("datetime_utc", "2024-01-02 03:04:05.123456+00:00", "timestamptz"),
        ("datetime_fixed", "2024-01-02 03:04:05.123456+02:30", "timestamptz"),
        ("datetime_local", "2024-01-02 03:04:05.123456+01:00", "timestamptz"),
        ("ipnetwork", "192.0.2.129/24", "inet"),
        ("mac_address", [0, 17, 34, 51, 68, 255], "macaddr"),
    ]
    result = [
        ("type-" + kind, values(kind, data, postgres))
        for kind, data, postgres in samples
    ]
    enum = {"kind": "enum", "schema": "fixture", "name": 'State" 雪'}
    result.append(("type-enum", values(enum, "O'Brien 雪", enum["name"], "fixture")))
    if not finite:
        result.append(("type-nan", values("f32", "7fc00001", "float4")))
    return result


def literal_cases():
    result = []
    for name, program in type_cases(finite=True):
        data = program.data()
        for node in data["nodes"]:
            if node["op"] == "expr.value":
                node["data"]["mode"] = "literal"
        result.append((name.replace("type-", "literal-"), Program.from_dict(data)))
    return result
