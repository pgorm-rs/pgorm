"""Independent DDL; enum names and labels bind into a private reference function."""

from dataclasses import dataclass

from . import wire
from .comparison import InvalidOracle
from .reference_sql import SQL, bound, join
from .reference_values import qualified, quote

# PostgreSQL utility statements do not accept parameters for enum labels. A
# private reference function receives bound data and uses the server's own
# identifier/literal quoting. It is removed before final state is observed.
ENUM_HELPER = """
CREATE FUNCTION FUNCTION_NAME(op text, s text, n text, labels text[], old text, new text)
RETURNS void LANGUAGE plpgsql AS $reference$
DECLARE label_list text;
BEGIN
  CASE op
    WHEN 'create' THEN
      SELECT string_agg(format('%L', label), ', ' ORDER BY position)
        INTO label_list FROM unnest(labels) WITH ORDINALITY AS t(label, position);
      EXECUTE format('CREATE TYPE %I.%I AS ENUM (%s)', s, n, coalesce(label_list, ''));
    WHEN 'add' THEN
      EXECUTE format('ALTER TYPE %I.%I ADD VALUE %L', s, n, old);
    WHEN 'rename' THEN
      EXECUTE format('ALTER TYPE %I.%I RENAME VALUE %L TO %L', s, n, old, new);
    WHEN 'drop' THEN
      EXECUTE format('DROP TYPE %I.%I', s, n);
    ELSE RAISE EXCEPTION 'unknown independent enum operation';
  END CASE;
END
$reference$
"""


@dataclass(frozen=True)
class DDL:
    statement: SQL


def column(definition):
    kind = definition["kind"]
    types = {
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
        "time": "time without time zone",
        "timestamp": "timestamp without time zone",
        "timestamptz": "timestamp with time zone",
        "text[]": "text[]",
        "i32[]": "integer[]",
    }
    type_name = (
        qualified(kind["enum"][1], kind["enum"][0])
        if isinstance(kind, dict)
        else types[kind]
    )
    return (
        quote(definition["name"])
        + " "
        + type_name
        + (" NOT NULL" if not definition["nullable"] else "")
        + (" PRIMARY KEY" if definition["primary"] else "")
    )


def schema_node(name, i, d, enum_function=None):
    if name in ("schema.enum", "schema.enum_change"):
        if enum_function is None:
            raise InvalidOracle("independent enum helper is unavailable")
        values = [
            wire.scalar("text", "create" if name == "schema.enum" else d["method"]),
            wire.scalar("text", d["schema"]),
            wire.scalar("text", d["name"]),
        ]
        labels = {
            "version": 1,
            "type": {"kind": "array", "element": {"kind": "text"}},
            "sql_null": False,
            "data": [wire.scalar("text", label) for label in d.get("labels", [])],
        }
        values += [
            labels,
            wire.scalar("text", d.get("value"), sql_null="value" not in d),
            wire.scalar("text", d.get("new_value"), sql_null="new_value" not in d),
        ]
        return DDL(
            "SELECT "
            + enum_function
            + "("
            + join([bound(value) for value in values])
            + ")"
        )
    table = i["table"]
    target = qualified(table.name, table.schema)
    match name:
        case "schema.create":
            statement = (
                "CREATE TABLE "
                + target
                + " ("
                + join([column(item) for item in d["columns"]])
                + ")"
            )
        case "schema.drop":
            statement = SQL(("DROP TABLE " + target,))
        case "schema.rename":
            item = "COLUMN " + quote(d["column"]) + " TO " if "column" in d else "TO "
            statement = SQL(
                ("ALTER TABLE " + target + " RENAME " + item + quote(d["name"]),)
            )
        case "schema.index":
            statement = (
                "CREATE "
                + ("UNIQUE " if d["unique"] else "")
                + "INDEX "
                + quote(d["name"])
                + " ON "
                + target
                + " ("
                + join([quote(name) for name in d["columns"]])
                + ")"
            )
        case _:
            raise InvalidOracle("independent schema semantics uncovered: " + name)
    return DDL(statement)
