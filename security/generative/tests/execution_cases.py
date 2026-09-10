"""Handwritten executor probes; these are not generated campaign programs."""

from pgorm_campaign import baseline, wire
from pgorm_campaign.program import Program


class Case:
    def __init__(self):
        self.nodes = []
        self.steps = []
        self.binders = []

    def node(self, op, inputs=None, data=None, *, scope="root"):
        identity = "n" + str(len(self.nodes))
        self.nodes.append(
            {
                "id": identity,
                "op": op,
                "inputs": inputs or {},
                "data": data or {},
                "scope": scope,
            }
        )
        return identity

    def step(self, op, inputs=None, data=None, *, scope="root"):
        identity = "s" + str(len(self.steps))
        self.steps.append(
            {
                "id": identity,
                "op": op,
                "inputs": inputs or {},
                "data": data or {},
                "scope": scope,
            }
        )
        return identity

    def value(self, kind, data):
        return self.node(
            "value",
            data={
                "value": wire.scalar(
                    kind, str(data) if kind in wire.INTEGER_BITS else data
                )
            },
        )

    def fetch(self, query, *, scope="root"):
        return self.step(
            "fetch", {"query": query}, {"mode": "all", "ordered": False}, scope=scope
        )

    def program(self):
        return Program.from_dict(
            {
                "version": 1,
                "capability_version": 1,
                "seed": 0,
                "fixture": baseline.default(),
                "binders": self.binders,
                "nodes": self.nodes,
                "steps": self.steps,
                "observations": [
                    {"step": step["id"], "oracle": "reference"} for step in self.steps
                ]
                + [{"step": "final", "oracle": "fixture-state"}],
            }
        )


def select_case():
    c = Case()
    table = c.node("table", data={"schema": "fixture", "name": "accounts"})
    column = c.node("expr.column", {"table": table}, {"name": "id"})
    tenant = c.node("expr.column", {"table": table}, {"name": "tenant"})
    value = c.value("i32", 1)
    predicate = c.node(
        "expr.binary", {"left": tenant, "right": value}, {"operator": "eq"}
    )
    query = c.node("select", {"columns": [column]})
    query = c.node("select.from", {"query": query, "table": table})
    query = c.node("select.filter", {"query": query, "predicate": predicate})
    c.fetch(query)
    return c.program()


def graph_case(name, aliases, *, cursor=False):
    c = Case()
    graph = c.node("graph", data={"name": name})
    query = c.node("graph.find", {"graph": graph}, {"aliases": aliases})
    if cursor:
        query = c.node("graph.cursor", {"query": query}, {"column": "rank"})
        query = c.node(
            "cursor.page",
            {"cursor": query},
            {"side": "first", "count": 2, "direction": "asc"},
        )
    c.fetch(query)
    return c.program()


def pipeline_case(*, bound, sources=False):
    c = Case()
    entity = c.node(
        "entity", data={"name": "campaign.Account" if sources else "campaign.Note"}
    )
    source = c.node("pipeline.source", {"source": entity}, {"alias": "n"})
    query = c.node("pipeline.from", {"source": source})
    column = c.node("pipeline.column", data={"source": "n", "column": "tenant"})
    value = c.value("i32", 1)
    expression = c.node(
        "pipeline.bind" if bound else "pipeline.value",
        {"value": value},
        scope="b" if bound else "root",
    )
    predicate = c.node(
        "pipeline.binary",
        {"left": column, "right": expression},
        {"operator": "eq"},
        scope="b" if bound else "root",
    )
    query = c.node(
        "pipeline.filter",
        {"query": query, "predicate": predicate},
        {"binder": "b"} if bound else {},
    )
    if bound:
        c.binders.append({"id": "b", "owner": query})
    if sources:
        query = c.node(
            "pipeline.sources",
            {"query": query},
            {"name": "campaign.Sources1", "qualifiers": ["n"]},
        )
    else:
        projection = c.node("pipeline.column", data={"source": "n", "column": "id"})
        query = c.node("pipeline.select", {"query": query, "columns": [projection]})
    c.fetch(query)
    return c.program()


def model_case():
    c = Case()
    table = c.node("table", data={"schema": "fixture", "name": "accounts"})
    model = c.node(
        "model",
        {"table": table},
        {"fields": {"identity": "id", "labels": "tags", "state": "state"}},
    )
    query = c.node("model.select", {"model": model, "order": []})
    c.fetch(query)
    return c.program()


def active_case():
    c = Case()
    entity = c.node("entity", data={"name": "campaign.Account"})
    query = c.node("entity.find", {"entity": entity})
    column = c.node("entity.column", {"entity": entity}, {"name": "id"})
    order = c.node(
        "expr.order", {"value": column}, {"direction": "asc", "nulls": "default"}
    )
    query = c.node("entity.order", {"query": query, "keys": [order]})
    query = c.node("entity.page", {"query": query}, {"limit": 1})
    read = c.fetch(query)
    model = c.node("entity.result", data={"step": read, "row": 0})
    active = c.node("entity.into_active", {"model": model})
    value = c.value("text", "O'Brien|updated")
    active = c.node(
        "active.set",
        {"model": active, "value": value},
        {"column": "name", "state": "set"},
    )
    c.step("active.write", {"model": active}, {"method": "update"})
    c.fetch(query)
    return c.program()


def transaction_case():
    c = Case()
    table = c.node("table", data={"schema": "fixture", "name": "notes"})
    value = c.value("text", "transaction body")
    query = c.node("update", {"table": table})
    query = c.node("update.set", {"query": query, "value": value}, {"column": "body"})
    query = c.node("write.all", {"query": query})
    c.step(
        "begin",
        data={"child": "tx", "mode": "read_write", "isolation": "read_committed"},
    )
    c.step(
        "begin",
        data={"child": "savepoint", "mode": "default", "isolation": "default"},
        scope="tx",
    )
    c.step("execute", {"query": query}, scope="savepoint")
    c.step("rollback", scope="savepoint")
    c.step("commit", scope="tx")
    entity = c.node("entity", data={"name": "campaign.Note"})
    read = c.node("entity.find", {"entity": entity})
    c.fetch(read)
    return c.program()


def schema_case():
    c = Case()
    table = c.node("table", data={"schema": "fixture", "name": 'new" 雪'})
    query = c.node(
        "schema.create",
        {"table": table},
        {
            "columns": [
                {"name": "id", "kind": "i32", "primary": True, "nullable": False},
                {"name": "name", "kind": "text", "primary": False, "nullable": True},
            ]
        },
    )
    c.step("execute", {"query": query})
    value = c.value("i32", 8)
    query = c.node("insert", {"table": table}, {"columns": ["id"]})
    query = c.node("insert.row", {"query": query, "values": [value]})
    column = c.node("expr.column", {"table": table}, {"name": "id"})
    query = c.node("write.returning", {"query": query, "columns": [column]})
    c.fetch(query)
    query = c.node("schema.drop", {"table": table})
    c.step("execute", {"query": query})
    return c.program()


def stream_case(*, take, cancel):
    c = Case()
    table = c.node("table", data={"schema": "fixture", "name": "notes"})
    column = c.node("expr.column", {"table": table}, {"name": "id"})
    query = c.node("select", {"columns": [column]})
    query = c.node("select.from", {"query": query, "table": table})
    c.step(
        "stream", {"query": query}, {"take": take, "cancel": cancel, "ordered": False}
    )
    c.fetch(query)
    return c.program()
