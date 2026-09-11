"""Check inspected read SQL against independent results without changing fixture data."""

import re

from . import wire
from .comparison import InvalidOracle
from .reference import Driver, Rejection, Resolution
from .reference_pipeline import Relation
from .reference_sql import SQL, Query
from .reference_template import Raw, template


def read_statement(query, operation):
    if isinstance(query, Raw) and re.match(r"\s*SELECT\b", query.text, re.I):
        return query.sql()
    if isinstance(query, Query) and query.kind == "select" and not query.shape:
        return query.sql()
    if isinstance(query, Relation) and not query.shape:
        return query.terminal()
    if isinstance(query, SQL) and operation == "condition":
        return "SELECT TRUE WHERE " + query
    if isinstance(query, SQL) and operation.startswith("expr."):
        return "SELECT " + query
    raise InvalidOracle(
        "inspection oracle currently requires a scalar expression or an untyped read query"
    )


def parameters(compiled, program):
    values, adaptations = [], []
    explicit_unsigned = any(
        node["op"] == "value" and node["data"]["value"]["type"]["kind"] == "u64"
        for node in program["nodes"]
    )
    for index, value in enumerate(compiled["parameters"]):
        if value["type"]["kind"] == "u64":
            if (
                explicit_unsigned
                or value["sql_null"]
                or not 0 <= int(value["data"]) <= 2**63 - 1
            ):
                raise InvalidOracle("uncovered unsigned inspection parameter semantics")
            adaptations.append(
                {
                    "index": index,
                    "from": "u64",
                    "to": "i64",
                    "reason": "PostgreSQL LIMIT/OFFSET parameter carrier",
                }
            )
            value = wire.scalar("i64", value["data"])
        values.append(value)
    return values, adaptations


async def inspect_reads(executor, program, subject):
    data = program.data()
    if not any(step["op"] == "inspect" for step in data["steps"]):
        return
    resolution = Resolution(data)
    for step in data["steps"]:
        if step["op"] not in ("fetch", "inspect", "stream") or step["scope"] != "root":
            raise InvalidOracle(
                "inspection in a program with writes or transactions needs an oracle at the inspection point"
            )
        node = resolution.nodes[step["inputs"]["query"]]
        read_statement(resolution.get(node["id"]), node["op"])
    async with await Driver.connect(
        executor.fixture, worker=executor.worker, side="subject"
    ) as driver:
        for step in subject["steps"]:
            if step["operation"] != "inspect" or step["observation"]["kind"] == "error":
                continue
            compiled = step["observation"]
            values, adaptations = parameters(compiled, data)
            try:
                async with driver.connection.transaction():
                    await driver.connection.execute("SET TRANSACTION READ ONLY")
                    records, _ = await driver.query(template(compiled["sql"], values))
                probe = {"kind": "rows", "rows": records}
            except Rejection as error:
                probe = error.observation
            step["inspection_probe"] = probe
            step["inspection_parameter_adaptations"] = adaptations
