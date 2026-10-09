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
    "daterange": "daterange",
}
TABLE_FIELDS = {"schema", "name", "columns", "rows"}
# A key `WITHOUT OVERLAPS` names its period; foreign keys are each a
# `FOREIGN KEY .. REFERENCES` clause of the table that declares them.
TABLE_OPTIONS = {"without_overlaps", "foreign_keys"}
FOREIGN_KEY_FIELDS = {
    "columns",
    "table",
    "references",
    "period",
    "enforced",
    "deferred",
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


def _names(values):
    if not isinstance(values, list) or not 1 <= len(values) <= 32:
        raise ValueError("fixture key columns must be a bounded nonempty list")
    return ",".join(identifier(value) for value in values)


def _foreign_key(key):
    """`FOREIGN KEY (..) REFERENCES ..` with PostgreSQL 18's optional PERIOD
    pair, `NOT ENFORCED`, and a check deferred to commit."""
    if not isinstance(key, dict) or set(key) != FOREIGN_KEY_FIELDS:
        raise ValueError("unexpected fixture foreign key fields")
    if type(key["enforced"]) is not bool or type(key["deferred"]) is not bool:
        raise ValueError("fixture foreign key flags must be booleans")
    source, target = _names(key["columns"]), _names(key["references"])
    if key["period"] is not None:
        if not isinstance(key["period"], list) or len(key["period"]) != 2:
            raise ValueError("a fixture foreign key period names two columns")
        source += ",PERIOD " + identifier(key["period"][0])
        target += ",PERIOD " + identifier(key["period"][1])
    if not isinstance(key["table"], list) or len(key["table"]) != 2:
        raise ValueError("a fixture foreign key names its table's schema and name")
    clause = (
        "FOREIGN KEY ("
        + source
        + ") REFERENCES "
        + qualified(*key["table"])
        + " ("
        + target
        + ")"
    )
    if key["deferred"]:
        clause += " DEFERRABLE INITIALLY DEFERRED"
    return clause + ("" if key["enforced"] else " NOT ENFORCED")


def _table_sql(table, enums):
    if not TABLE_FIELDS <= set(table) <= TABLE_FIELDS | TABLE_OPTIONS:
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
    period = table.get("without_overlaps")
    if period is not None:
        if not primary or primary[-1] != identifier(period) or len(primary) < 2:
            raise ValueError(
                "WITHOUT OVERLAPS names the last of two or more key columns"
            )
        primary[-1] += " WITHOUT OVERLAPS"
    if primary:
        definitions.append("PRIMARY KEY (" + ",".join(primary) + ")")
    for key in table.get("foreign_keys", []):
        definitions.append(_foreign_key(key))
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


def extensions(tables):
    """btree_gist, in the fixture's own schema, when a key `WITHOUT OVERLAPS`
    has a scalar part: its GiST operator classes are what such a key indexes
    the scalar with. It is trusted, so the campaign role installs it, and the
    schema's reset drops it again."""
    if any("without_overlaps" in table for table in tables):
        return ['CREATE EXTENSION IF NOT EXISTS btree_gist SCHEMA "fixture";']
    return []


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
    if any(
        schema not in ("fixture", "other") and not schema.startswith("campaign_")
        for schema in schemas
    ):
        raise ValueError("fixture schemas must use the owned campaign namespace")
    setup = [
        "SET standard_conforming_strings = on;",
        "BEGIN;",
        "DO $reset$ DECLARE owned text; BEGIN "
        "FOR owned IN SELECT nspname FROM pg_catalog.pg_namespace "
        "WHERE nspowner = 'campaign'::regrole LOOP "
        "EXECUTE pg_catalog.format('DROP SCHEMA %I CASCADE', owned); "
        "END LOOP; END $reset$;",
    ]
    for schema in sorted(schemas | {"fixture", "other"}):
        setup.append("CREATE SCHEMA " + identifier(schema) + " AUTHORIZATION campaign;")
    return "\n".join(
        setup
        + ["SET LOCAL ROLE campaign;"]
        + extensions(tables)
        + enum_sql
        + statements
        + ["COMMIT;"]
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
