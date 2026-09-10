"""Ordered public asynchronous terminals and explicit transaction/stream cleanup."""

import asyncio

from . import observations


async def fetch(query, mode, connection, p):
    from pgorm.models.queries import ModelRows
    from pgorm.pipeline import Pipeline, SelectedSources

    method = "one_opt" if mode == "optional" else mode
    for category, prefix in (
        (p.EntityQuery, "pgorm::Select"),
        (p.GraphQuery, "pgorm::SelectGraph"),
        (p.GraphCursor, "pgorm::Cursor"),
        (Pipeline, "pgorm::pipeline::Pipeline"),
        (SelectedSources, "pgorm::pipeline::SelectedSources"),
        (ModelRows, "pgorm::ConnectionTrait::query_raw"),
    ):
        if isinstance(query, category):
            if not hasattr(query, method):
                raise p.UnsupportedCapabilityError(
                    "query has no public " + method + " terminal"
                )
            value = await getattr(query, method)(connection)
            path = prefix if category is ModelRows else prefix + "::" + method
            break
    else:
        value = await getattr(connection, "fetch_" + mode)(query)
        path = "pgorm::ConnectionTrait::query_raw"
    rows = value if mode == "all" else ([] if value is None else [value])
    return rows, [path]


async def stream(query, data, connection):
    records = []
    cancelled = False
    complete = False
    async with await connection.stream(query) as stream:
        for _ in range(data["take"]):
            try:
                records.append(await anext(stream))
            except StopAsyncIteration:
                complete = True
                break
        if data["cancel"] and not complete:
            pending = asyncio.create_task(anext(stream))
            try:
                await asyncio.sleep(0)
                pending.cancel()
                try:
                    await pending
                except asyncio.CancelledError:
                    cancelled = True
                except StopAsyncIteration:
                    complete = True
            finally:
                if not pending.done():
                    pending.cancel()
                    await asyncio.gather(pending, return_exceptions=True)
    return records, {
        "complete": complete,
        "cancelled": cancelled,
        "closed": stream.closed,
    }


class Effects:
    def __init__(self, resolution, connection):
        self.resolution = resolution
        self.p = resolution.p
        self.scopes = {"root": connection}
        self.open_transactions = []

    async def perform(self, step):
        name, d = step["op"], step["data"]
        inputs = {key: self.resolution.get(ref) for key, ref in step["inputs"].items()}
        connection = self.scopes[step["scope"]]
        query = inputs.get("query")
        match name:
            case "fetch":
                rows, paths = await fetch(query, d["mode"], connection, self.p)
                self.resolution.results[step["id"]] = rows
                return {
                    "kind": "rows",
                    "rows": [observations.row(row) for row in rows],
                }, paths
            case "execute":
                from pgorm.models.queries import ModelWrite

                value = (
                    await query.execute(connection)
                    if isinstance(query, ModelWrite)
                    else await connection.execute(query)
                )
                return {"kind": "count", "value": value}, [
                    "pgorm::ConnectionTrait::execute_raw"
                ]
            case "active.write":
                value = await getattr(inputs["model"], d["method"])(connection)
                result = (
                    {"kind": "count", "value": value}
                    if d["method"] == "delete"
                    else {"kind": "rows", "rows": [observations.row(value)]}
                )
                self.resolution.results[step["id"]] = (
                    [] if d["method"] == "delete" else [value]
                )
                return result, ["pgorm::ActiveModelTrait::" + d["method"]]
            case "begin":
                if step["scope"] == "root":
                    isolation = None if d["isolation"] == "default" else d["isolation"]
                    child = await connection.begin(mode=d["mode"], isolation=isolation)
                else:
                    child = await connection.begin()
                self.scopes[d["child"]] = child
                self.open_transactions.append(d["child"])
                return {"kind": "transaction", "scope": d["child"]}, [
                    "pgorm::TransactionTrait::begin_with"
                    if step["scope"] == "root"
                    else "pgorm::TransactionTrait::begin"
                ]
            case "commit" | "rollback":
                await getattr(connection, name)()
                self.open_transactions.remove(step["scope"])
                return {"kind": "unit"}, ["pgorm::DatabaseTransaction::" + name]
            case "stream":
                rows, state = await stream(query, d, connection)
                self.resolution.results[step["id"]] = rows
                return {
                    "kind": "rows",
                    "rows": [observations.row(row) for row in rows],
                    "stream": state,
                }, ["pgorm::ConnectionTrait::query_raw", "tokio_postgres::RowStream"]
            case "inspect":
                if not hasattr(query, "inspect"):
                    raise self.p.UnsupportedCapabilityError(
                        "query has no public inspect terminal"
                    )
                from pgorm.pipeline import Pipeline, SelectedSources

                prefix = (
                    "pgorm::pipeline::Pipeline::into_sql"
                    if isinstance(query, (Pipeline, SelectedSources))
                    else "pgorm_query::QueryStatementBuilder"
                )
                return observations.compiled(query.inspect()), [prefix]
        raise RuntimeError("inactive effect dispatch: " + name)

    async def close(self):
        errors = []
        for identity in reversed(self.open_transactions):
            try:
                await self.scopes[identity].close()
            except Exception as error:
                errors.append(observations.error(error))
        self.open_transactions.clear()
        return errors
