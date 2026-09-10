"""Portable fixture data rendered independently of the pgorm subject renderer."""

import hashlib
import json
import math


KINDS = {
    "i16": "smallint",
    "i32": "integer",
    "i64": "bigint",
    "f32": "real",
    "f64": "double precision",
    "bool": "boolean",
    "text": "text",
    "bytes": "bytea",
    "decimal": "numeric",
    "json": "jsonb",
    "uuid": "uuid",
    "date": "date",
    "time": "time",
    "timestamp": "timestamp",
    "timestamptz": "timestamptz",
    "text[]": "text[]",
    "i32[]": "integer[]",
}


def identifier(value):
    if not isinstance(value, str) or not value or "\x00" in value:
        raise ValueError("fixture names must be nonempty strings without NUL")
    if len(value.encode("utf-8")) > 63:
        raise ValueError("fixture names exceed PostgreSQL's identifier limit")
    return '"' + value.replace('"', '""') + '"'


def text(value):
    if not isinstance(value, str) or "\x00" in value:
        raise ValueError("fixture text must be a string without NUL")
    return "'" + value.replace("'", "''") + "'"


def qualified(schema, name):
    return identifier(schema) + "." + identifier(name)


def type_sql(kind, enums):
    if isinstance(kind, str) and kind in KINDS:
        return KINDS[kind]
    if isinstance(kind, dict) and set(kind) == {"enum"}:
        identity = kind["enum"]
        if isinstance(identity, list) and len(identity) == 2:
            name = qualified(*identity)
            if name in enums:
                return name
    raise ValueError("unknown fixture column kind")


def value_sql(value, kind, enums):
    sql_type = type_sql(kind, enums)
    if value is None:
        return "NULL::" + sql_type
    if kind == "json":
        encoded = json.dumps(value, ensure_ascii=False, allow_nan=False)
    elif isinstance(kind, str) and kind.endswith("[]"):
        if not isinstance(value, list) or len(value) > 256:
            raise ValueError("fixture arrays must be bounded lists")
        items = [value_sql(item, kind[:-2], enums) for item in value]
        return "ARRAY[" + ",".join(items) + "]::" + sql_type
    elif isinstance(value, bool):
        encoded = "true" if value else "false"
    elif isinstance(value, (str, int, float)):
        if isinstance(value, float) and not math.isfinite(value):
            raise ValueError("fixture floats must use explicit textual special values")
        encoded = str(value)
    else:
        raise ValueError("fixture scalar has an unsupported representation")
    return text(encoded) + "::" + sql_type


def _table_sql(table, enums):
    if set(table) != {"schema", "name", "columns", "rows"}:
        raise ValueError("unexpected fixture table fields")
    name = qualified(table["schema"], table["name"])
    columns = table["columns"]
    rows = table["rows"]
    if not isinstance(columns, list) or not 1 <= len(columns) <= 32:
        raise ValueError("fixture column count must be between 1 and 32")
    if not isinstance(rows, list) or len(rows) > 256:
        raise ValueError("fixture row count must be at most 256")
    definitions, names, primary = [], [], []
    for column in columns:
        if set(column) != {"name", "kind", "nullable", "primary"}:
            raise ValueError("unexpected fixture column fields")
        column_name = identifier(column["name"])
        if column_name in names:
            raise ValueError("duplicate fixture column")
        if type(column["nullable"]) is not bool or type(column["primary"]) is not bool:
            raise ValueError("fixture column flags must be booleans")
        names.append(column_name)
        definition = column_name + " " + type_sql(column["kind"], enums)
        definitions.append(definition + ("" if column["nullable"] else " NOT NULL"))
        if column["primary"]:
            primary.append(column_name)
    if primary:
        definitions.append("PRIMARY KEY (" + ",".join(primary) + ")")
    statements = ["CREATE TABLE " + name + " (" + ",".join(definitions) + ");"]
    if rows:
        rendered = []
        for row in rows:
            if not isinstance(row, list) or len(row) != len(columns):
                raise ValueError("fixture row does not match its declared columns")
            if any(v is None and not c["nullable"] for v, c in zip(row, columns)):
                raise ValueError("fixture NULL violates its column declaration")
            items = [value_sql(v, c["kind"], enums) for v, c in zip(row, columns)]
            rendered.append("(" + ",".join(items) + ")")
        statements.append("INSERT INTO " + name + " VALUES " + ",".join(rendered) + ";")
    return statements


# [spec:pgorm:req:generative.fixtures]
def render(definition):
    """Validate and render a bounded fixture; never run its contents on loading."""
    if not isinstance(definition, dict) or set(definition) != {
        "version",
        "enums",
        "tables",
    }:
        raise ValueError("unexpected fixture definition fields")
    if type(definition["version"]) is not int or definition["version"] != 1:
        raise ValueError("unsupported fixture version")
    encoded = json.dumps(definition, ensure_ascii=False, allow_nan=False)
    if len(encoded.encode("utf-8")) > 2**20:
        raise ValueError("fixture definition exceeds its byte budget")
    tables, definitions = definition["tables"], definition["enums"]
    if not isinstance(tables, list) or not 1 <= len(tables) <= 16:
        raise ValueError("fixture table count must be between 1 and 16")
    if not isinstance(definitions, list) or len(definitions) > 16:
        raise ValueError("fixture enum count must be at most 16")
    schemas = set()
    enums = set()
    enum_sql = []
    for enum in definitions:
        if set(enum) != {"schema", "name", "labels"}:
            raise ValueError("unexpected fixture enum fields")
        name = qualified(enum["schema"], enum["name"])
        labels = enum["labels"]
        if not isinstance(labels, list) or not 1 <= len(labels) <= 64:
            raise ValueError("fixture enums must have bounded nonempty labels")
        if name in enums or len(set(labels)) != len(labels):
            raise ValueError("duplicate fixture enum or label")
        enums.add(name)
        schemas.add(enum["schema"])
        enum_sql.append(
            "CREATE TYPE " + name + " AS ENUM (" + ",".join(map(text, labels)) + ");"
        )
    names = set()
    statements = []
    for table in tables:
        name = qualified(table["schema"], table["name"])
        if name in names:
            raise ValueError("duplicate fixture table")
        names.add(name)
        schemas.add(table["schema"])
        statements.extend(_table_sql(table, enums))
    if not schemas <= {"fixture", "other"}:
        raise ValueError("fixture schemas must be fixture or other")
    setup = ["SET standard_conforming_strings = on;", "BEGIN;"]
    for schema in ("fixture", "other"):
        setup.extend(
            [
                "DROP SCHEMA IF EXISTS " + identifier(schema) + " CASCADE;",
                "CREATE SCHEMA " + identifier(schema) + " AUTHORIZATION campaign;",
            ]
        )
    return "\n".join(
        setup + ["SET LOCAL ROLE campaign;"] + enum_sql + statements + ["COMMIT;"]
    )


def digest(definition):
    render(definition)
    data = json.dumps(
        definition, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    )
    return hashlib.sha256(data.encode("utf-8")).hexdigest()


def restore(definition):
    """Restore rows without replacing enum identities or prepared query types."""
    render(definition)
    enums = {qualified(enum["schema"], enum["name"]) for enum in definition["enums"]}
    statements = [
        "SET standard_conforming_strings = on;",
        "BEGIN;",
        "SET LOCAL ROLE campaign;",
    ]
    names = [
        qualified(table["schema"], table["name"]) for table in definition["tables"]
    ]
    statements.append("TRUNCATE " + ",".join(names) + ";")
    for table in definition["tables"]:
        statements.extend(_table_sql(table, enums)[1:])
    return "\n".join(statements + ["COMMIT;"])


def column(name, kind, *, nullable=False, primary=False):
    return {"name": name, "kind": kind, "nullable": nullable, "primary": primary}


def default():
    """Independent tenants, missing joins, duplicate ranks and hostile names."""
    account_columns = [
        column("id", "i32", primary=True),
        column("tenant", "i32"),
        column("name", "text"),
        column("note", "text", nullable=True),
        column("score", "i32", nullable=True),
        column("rank", "i32"),
        column("active", "bool"),
        column("balance", "decimal"),
        column("tags", "text[]"),
        column("payload", "json"),
        column("uuid", "uuid"),
        column("created_at", "timestamp"),
        column("occurred_at", "timestamptz"),
        column("event_date", "date"),
        column("event_time", "time"),
        column("state", {"enum": ["fixture", 'State" 雪']}),
    ]
    accounts = []
    for identity, tenant, name, note, score, rank in (
        (1, 1, "Alice", None, 10, 1),
        (2, 1, "O'Brien 雪", "literal %_\\", 10, 1),
        (3, 2, "Sentinel", "protected tenant", 30, 2),
        (4, 1, "No notes", None, None, 2),
    ):
        accounts.append(
            [
                identity,
                tenant,
                name,
                note,
                score,
                rank,
                identity != 4,
                "123.4500",
                [name, None, "%_"],
                {"owner": name, "null": None},
                f"00000000-0000-0000-0000-{identity:012d}",
                "2024-01-02 03:04:05.123456",
                "2024-01-02 03:04:05.123456+00",
                "2024-01-02",
                "03:04:05.123456",
                "calm",
            ]
        )
    tables = [
        {
            "schema": "fixture",
            "name": "accounts",
            "columns": account_columns,
            "rows": accounts,
        }
    ]
    tables.append(
        {
            "schema": "fixture",
            "name": "notes",
            "columns": [
                column("id", "i32", primary=True),
                column("account_id", "i32"),
                column("tenant", "i32"),
                column("body", "text"),
            ],
            "rows": [
                [11, 1, 1, "first"],
                [12, 1, 1, "second"],
                [13, 3, 2, "protected"],
                [14, 99, 1, "orphan"],
            ],
        }
    )
    for schema, name, rows in (
        ("other", "accounts", [[1, "schema collision"]]),
        ("fixture", 'odd" 雪', [[1, "O'Brien; -- $tag$ \\"]]),
        ("fixture", "sentinels", [[1, "never change"], [2, "other tenant"]]),
    ):
        tables.append(
            {
                "schema": schema,
                "name": name,
                "columns": [
                    column("id", "i32", primary=True),
                    column("name", "text"),
                ],
                "rows": rows,
            }
        )
    return {
        "version": 1,
        "enums": [
            {"schema": "fixture", "name": 'State" 雪', "labels": ["calm", "O'Brien 雪"]}
        ],
        "tables": tables,
    }
