"""Execute portable programs in the independent fixture using Psycopg."""

import asyncio
from dataclasses import replace
import time
import uuid

from .comparison import InvalidOracle
from .reference_active import active_node, write as active_write
from .reference_codec import Codec
from .reference_cursor import cursor_node, prepare as cursor_prepare
from .reference_expr import scalar_node
from .reference_models import model_node, reshape
from .reference_pipeline import Relation, pipeline_node
from .reference_schema import DDL, ENUM_HELPER, schema_node
from .reference_sql import SQL, Query, query_node
from .reference_template import Raw, validate_types
from .reference_values import install, qualified, quote


class Rejection(Exception):
    def __init__(self, category, cause, sqlstate=None):
        self.observation = {
            "kind": "error",
            "class": category,
            "cause": cause,
            "sqlstate": sqlstate,
        }
        super().__init__(cause)


class Resolution:
    def __init__(self, program, enum_function=None):
        self.program = program
        self.nodes = {node["id"]: node for node in program["nodes"]}
        self.cache = {}
        self.results = {}
        self.enum_function = enum_function

    def get(self, identity):
        if identity in self.cache:
            return self.cache[identity]
        node = self.nodes[identity]
        inputs = {
            key: [self.get(ref) for ref in value]
            if isinstance(value, list)
            else self.get(value)
            for key, value in node["inputs"].items()
        }
        name, data = node["op"], node["data"]
        if name == "raw.template":
            result = Raw(data["text"], inputs["parameters"])
        elif name.startswith("pipeline."):
            result = pipeline_node(name, inputs, data, self.program["fixture"])
        elif name.startswith("schema."):
            result = schema_node(name, inputs, data, self.enum_function)
        elif name == "result.value":
            effect = next(
                step for step in self.program["steps"] if step["id"] == data["step"]
            )
            if effect["op"] == "stream" and not effect["data"]["ordered"]:
                raise InvalidOracle(
                    "stream result references require declared ordering"
                )
            row = self.results[data["step"]][data["row"]]
            if row["kind"] != "record":
                raise InvalidOracle("reference result value requires a record")
            result = next(
                item["value"]
                for item in row["fields"]
                if item["name"] == data["column"]
            )
            if result["type"] != data["type"]:
                raise InvalidOracle(
                    "reference result tag differs from the program declaration"
                )
        elif name in (
            "entity.active",
            "entity.result",
            "entity.into_active",
            "active.set",
        ):
            result = active_node(name, inputs, data, self.results)
        elif name == "graph.cursor" or name.startswith("cursor."):
            result = cursor_node(name, inputs, data)
        elif name.startswith("expr.") or name in (
            "value",
            "name",
            "table",
            "condition",
        ):
            result = scalar_node(name, inputs, data)
        elif name.startswith("model.") or name in (
            "entity",
            "entity.column",
            "entity.predicate",
            "entity.find",
            "model",
            "graph",
            "graph.find",
            "graph.column",
        ):
            result = model_node(name, inputs, data, self.program["fixture"])
        else:
            result = query_node(name, inputs, data)
        self.cache[identity] = result
        return result


class Driver:
    def __init__(self, connection):
        self.connection = connection
        self.codec = Codec(connection)
        install(connection)

    @classmethod
    async def connect(cls, fixture, *, worker=0, side="reference"):
        import psycopg

        if side not in ("subject", "reference"):
            raise ValueError("unknown owned fixture side")
        pair = fixture.pair(worker)
        connection = await psycopg.AsyncConnection.connect(
            getattr(pair, side), autocommit=True, connect_timeout=5
        )
        driver = cls(connection)
        try:
            await driver.codec.refresh()
        except BaseException:
            await connection.close()
            raise
        return driver

    async def query(self, sql, *, decode=True):
        import psycopg

        text, parameters = sql.command()
        try:
            async with self.connection.cursor(binary=True) as cursor:
                await cursor.execute(text, parameters, prepare=False)
                result = cursor.pgresult
                if result is None:
                    raise InvalidOracle("independent driver returned no command result")
                count = result.command_tuples or 0
                records = await self.codec.records(result) if decode else []
                return records, count
        except psycopg.Error as error:
            if error.sqlstate is None:
                raise InvalidOracle(
                    "independent driver failed without a server rejection"
                ) from error
            raise Rejection(
                "DatabaseError", error.diag.message_primary, error.sqlstate
            ) from error

    async def close(self):
        await self.connection.close()

    async def __aenter__(self):
        return self

    async def __aexit__(self, *error):
        await self.close()


class Reference:
    def __init__(self, driver):
        self.driver = driver
        self.transactions = []

    async def transaction(self, step):
        from .reference_sql import SQL

        name, data, scope = step["op"], step["data"], step["scope"]
        if name == "begin":
            if scope == "root":
                text = "BEGIN"
                if data["isolation"] != "default":
                    text += (
                        " ISOLATION LEVEL "
                        + data["isolation"].replace("_", " ").upper()
                    )
                if data["mode"] != "default":
                    text += " " + data["mode"].replace("_", " ").upper()
            else:
                text = "SAVEPOINT " + quote(data["child"])
            await self.driver.query(SQL((text,)), decode=False)
            self.transactions.append(data["child"])
            return {"kind": "transaction", "scope": data["child"]}
        if not self.transactions or self.transactions[-1] != scope:
            raise InvalidOracle(
                "reference transaction stack does not match the program"
            )
        if len(self.transactions) == 1:
            await self.driver.query(SQL((name.upper(),)), decode=False)
        else:
            if name == "rollback":
                await self.driver.query(
                    SQL(("ROLLBACK TO SAVEPOINT " + quote(scope),)), decode=False
                )
            await self.driver.query(
                SQL(("RELEASE SAVEPOINT " + quote(scope),)), decode=False
            )
        self.transactions.pop()
        return {"kind": "unit"}

    async def perform(self, step, resolution):
        name, data = step["op"], step["data"]
        if name in ("begin", "commit", "rollback"):
            return await self.transaction(step)
        if name == "inspect":
            from .reference_inspection import read_statement

            identity = step["inputs"]["query"]
            query = resolution.get(identity)
            statement = read_statement(query, resolution.nodes[identity]["op"])
            async with self.driver.connection.transaction():
                await self.driver.connection.execute("SET TRANSACTION READ ONLY")
                if isinstance(query, Raw):
                    await validate_types(self.driver, query)
                rows, _ = await self.driver.query(statement)
            return {"kind": "rows", "rows": rows}
        if name not in ("fetch", "execute", "stream", "active.write"):
            raise InvalidOracle("independent effect semantics uncovered: " + name)
        query = (
            active_write(resolution.get(step["inputs"]["model"]), data["method"])
            if name == "active.write"
            else resolution.get(step["inputs"]["query"])
        )
        if isinstance(query, DDL):
            if name != "execute":
                raise InvalidOracle("DDL oracle requires an execute effect")
            await self.driver.query(query.statement, decode=False)
            return {"kind": "count", "value": 0}
        if isinstance(query, Raw):
            await validate_types(self.driver, query)
            query = query.sql()
        if not isinstance(query, (Query, Relation, SQL)):
            raise InvalidOracle("reference effect requires an independent query")
        mode = data.get("mode", "all")
        shape = query.shape if isinstance(query, (Query, Relation)) else {}
        if isinstance(query, Relation) and data.get("ordered") and not query.ordering:
            raise InvalidOracle(
                "ordered pipeline observation requires a surviving explicit sort"
            )
        if "cursor" in shape:
            query = cursor_prepare(query)
        limited = (
            (shape.get("model") is not None and shape["model"].entity is not None)
            or "slots" in shape
            or isinstance(query, Relation)
        )
        if mode != "all" and limited:
            query = (
                query.wrap(query.projected(), page=SQL((" LIMIT 1",)))
                if isinstance(query, Relation)
                else replace(query, limit=1)
            )
        statement = (
            query.terminal()
            if isinstance(query, Relation)
            else query.sql()
            if isinstance(query, Query)
            else query
        )
        records, count = await self.driver.query(statement, decode=name != "execute")
        if name == "execute" or name == "active.write" and data["method"] == "delete":
            return {"kind": "count", "value": count}
        if (
            mode == "one"
            and len(records) != 1
            or mode == "optional"
            and len(records) > 1
        ):
            qualifier = "exactly one" if mode == "one" else "at most one"
            raise Rejection(
                "DatabaseError", f"expected {qualifier} row, received {len(records)}"
            )
        records = reshape(records, shape)
        if "cursor" in shape and shape["cursor"]["side"] == "last":
            records.reverse()
        observation = {"kind": "rows", "rows": records}
        if name == "stream":
            observation = {
                "kind": "rows",
                "rows": records,
                "stream_check": {"take": data["take"], "cancel": data["cancel"]},
            }
        resolution.results[step["id"]] = (
            records[: data["take"]] if name == "stream" else records
        )
        return observation

    # [spec:pgorm:req:generative.oracles]
    async def run(self, program, *, timeout=10):
        started = time.monotonic()
        has_enums = any(
            node["op"] in ("schema.enum", "schema.enum_change")
            for node in program.data()["nodes"]
        )
        helper = (
            qualified("campaign_oracle_" + uuid.uuid4().hex, "fixture")
            if has_enums
            else None
        )
        resolution = Resolution(program.data(), helper)
        helper_created = False
        report = {
            "program_sha256": program.digest,
            "status": "running",
            "steps": [],
            "cleanup_errors": [],
        }
        try:
            async with asyncio.timeout(timeout):
                if helper is not None:
                    await self.driver.connection.execute(
                        ENUM_HELPER.replace("FUNCTION_NAME", helper)
                    )
                    helper_created = True
                for step in program.data()["steps"]:
                    try:
                        observation = await self.perform(step, resolution)
                    except Rejection as error:
                        observation = error.observation
                    report["steps"].append(
                        {"id": step["id"], "observation": observation}
                    )
                report["status"] = "executed"
        except Exception as error:
            report.update(
                status="incomplete",
                error={"class": type(error).__name__, "cause": str(error)},
            )
        finally:
            if self.transactions:
                try:
                    await self.driver.connection.execute("ROLLBACK")
                    self.transactions.clear()
                except Exception as error:
                    report["cleanup_errors"].append(
                        {"class": type(error).__name__, "cause": str(error)}
                    )
                    report["status"] = "incomplete"
            if helper_created:
                try:
                    await self.driver.connection.execute(
                        "DROP FUNCTION " + helper + "(text,text,text,text[],text,text)"
                    )
                except Exception as error:
                    report["cleanup_errors"].append(
                        {"class": type(error).__name__, "cause": str(error)}
                    )
                    report["status"] = "incomplete"
            report["seconds"] = time.monotonic() - started
        return report
