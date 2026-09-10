"""Deliberately incorrect SQL/results used only inside owned campaign fixtures."""

import copy

from .comparison import InvalidOracle
from .control_programs import IDENTIFIER, PAYLOAD
from .reference_sql import SQL, Parameter
from .wire import scalar


def observation(report, identity):
    report = copy.deepcopy(report)
    value = report["steps"][0]["observation"]
    if identity == "affected-count":
        value["value"] += 1
    elif identity == "error-cause":
        value["sqlstate"] = "23505"
    elif identity == "stream-close":
        value["stream"]["closed"] = False
    elif identity in ("missing-row", "duplicate-row", "row-order"):
        if identity == "missing-row":
            value["rows"].pop()
        elif identity == "duplicate-row":
            value["rows"].append(copy.deepcopy(value["rows"][0]))
        else:
            value["rows"].reverse()
    elif identity == "optional-slot":
        row = next(row for row in value["rows"] if row["items"][-1]["kind"] == "record")
        row["items"][-1] = {"kind": "absent"}
    else:
        typed(value["rows"][0]["fields"][0]["value"], identity)
    return report


def typed(value, identity):
    if identity == "decode-type":
        value["type"]["kind"] = "i64"
    elif identity == "json-null":
        value["sql_null"] = True
    elif identity == "array-element":
        value["data"].pop(1)
    elif identity == "enum-identity":
        value["type"]["schema"] = "other"
    elif identity == "float":
        value["data"] = "00000000"
    elif identity == "decimal":
        value["data"] = "123.45"
    elif identity == "temporal":
        value["data"] = "2024-01-02 03:04:05"
    else:
        raise InvalidOracle("unknown observation control: " + identity)


async def postgres(driver, report, identity):
    report, commands = copy.deepcopy(report), []
    if identity in ("missing-write", "missing-schema", "rollback"):
        if identity == "rollback":
            statements = [
                SQL(("BEGIN",)),
                SQL(
                    (
                        "UPDATE fixture.accounts SET name = ",
                        Parameter(scalar("text", "control changed")),
                        " WHERE tenant = 1",
                    )
                ),
                SQL(("COMMIT",)),
            ]
        else:
            # A deliberately fabricated successful count with no state effect.
            statements = [SQL(("SELECT 1",))]
        for statement in statements:
            await driver.query(statement, decode=False)
            commands.append(statement.command()[0])
    else:
        statement = read_statement(identity)
        records, _ = await driver.query(statement)
        report["steps"][0]["observation"] = {"kind": "rows", "rows": records}
        commands.append(statement.command()[0])
    return report, commands


def read_statement(identity):
    if identity == "escaped-value":
        # Intentionally missing value escaping; data remains fixture-local.
        return SQL(
            (
                "SELECT id FROM fixture.accounts WHERE tenant = 1 AND name = '"
                + PAYLOAD
                + "' ORDER BY id",
            )
        )
    if identity == "identifier":
        # Intentionally missing identifier escaping, independently of pgorm.
        return SQL(
            (
                'SELECT id AS "'
                + IDENTIFIER
                + '" FROM fixture.accounts WHERE tenant = 1 ORDER BY id',
            )
        )
    if identity == "predicate":
        return SQL(("SELECT id FROM fixture.accounts ORDER BY id",))
    if identity == "bind-order":
        return SQL(
            (
                "SELECT id FROM fixture.accounts WHERE tenant = ",
                Parameter(scalar("i32", "2")),
                "::int4 AND id = ",
                Parameter(scalar("i32", "1")),
                "::int4 ORDER BY id",
            )
        )
    if identity == "bind-type":
        return SQL(("SELECT ", Parameter(scalar("i32", "42")), "::int4 AS value"))
    raise InvalidOracle("unknown PostgreSQL control: " + identity)
