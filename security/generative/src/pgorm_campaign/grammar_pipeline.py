"""Typed pipeline transitions, including owned callbacks and nested sources."""

from dataclasses import dataclass, replace


@dataclass(frozen=True)
class Column:
    name: str
    kind: str
    nullable: bool = False
    source: str | None = None


# [spec:pgorm:req:generative.grammar]
class Pipeline:
    def __init__(self, state):
        self.state = state
        self.alias = state.name("origin")
        table = state.node("table", data={"schema": "fixture", "name": "accounts"})
        source = state.node("pipeline.source", {"source": table}, {"alias": self.alias})
        self.query = state.node("pipeline.from", {"source": source})
        self.columns = [
            Column(name, kind, nullable, self.alias)
            for name, kind, nullable in (
                ("id", "i32", False),
                ("tenant", "i32", False),
                ("name", "text", False),
                ("score", "i32", True),
                ("rank", "i32", False),
            )
        ]
        self.project(self.columns, prefix="p_")
        nonce = self.constant("i64", state.index, "root")
        nonce = state.node("pipeline.named", {"value": nonce}, {"name": "nonce"})
        self.query = state.node(
            "pipeline.derive", {"query": self.query, "columns": [nonce]}
        )
        self.columns.append(Column("nonce", "i64"))

    def expression(self, column, scope="root"):
        if column not in self.columns:
            raise ValueError("pipeline expression references an unavailable source")
        if column.source is None:
            return self.state.node(
                "pipeline.alias", data={"name": column.name}, scope=scope
            )
        return self.state.node(
            "pipeline.column",
            data={"source": column.source, "column": column.name},
            scope=scope,
        )

    def constant(self, kind, value, scope):
        state = self.state
        value = state.value(kind, value)
        value = state.node(
            "pipeline.value" if scope == "root" else "pipeline.bind",
            {"value": value},
            scope=scope,
        )
        return state.node(
            "pipeline.cast",
            {"value": value},
            {"name": {"text": "text", "i32": "integer", "i64": "bigint"}[kind]},
            scope=scope,
        )

    def scope(self):
        return (
            "b" + str(len(self.state.author.binders))
            if self.state.choices.take((False, True))
            else "root"
        )

    def stage(self, operation, inputs, scope="root", data=None):
        state = self.state
        data = dict(data or {})
        if scope != "root":
            data["binder"] = scope
        self.query = state.node(operation, {"query": self.query, **inputs}, data)
        if scope != "root":
            state.author.binders.append({"id": scope, "owner": self.query})

    def predicate(self, scope, depth):
        state = self.state
        if depth and state.choices.take((False, True)):
            left, right = (
                self.predicate(scope, depth - 1),
                self.predicate(scope, depth - 1),
            )
            return state.node(
                "pipeline.binary",
                {"left": left, "right": right},
                {"operator": state.choices.take(("and", "or"))},
                scope=scope,
            )
        column = state.choices.take(self.columns)
        left = self.expression(column, scope)
        right = self.constant(
            column.kind,
            state.text() if column.kind == "text" else state.choices.integer(-2, 40),
            scope,
        )
        result = state.node(
            "pipeline.binary",
            {"left": left, "right": right},
            {"operator": state.choices.take(("eq", "ne", "lt", "lte", "gt", "gte"))},
            scope=scope,
        )
        if state.choices.take((False, True)):
            result = state.node(
                "pipeline.unary", {"value": result}, {"operator": "not"}, scope=scope
            )
        return result

    def filter(self):
        scope = self.scope()
        self.stage(
            "pipeline.filter",
            {"predicate": self.predicate(scope, self.state.limits.depth)},
            scope,
        )

    def project(self, columns=None, *, prefix=""):
        state = self.state
        if columns is None:
            columns = [
                column
                for column in self.columns
                if column.name == "nonce" or state.choices.take((False, True))
            ]
        if not columns:
            columns = [state.choices.take(self.columns)]
        expressions = [
            state.node(
                "pipeline.named",
                {"value": self.expression(column)},
                {"name": prefix + column.name},
            )
            for column in columns
        ]
        self.stage("pipeline.select", {"columns": expressions})
        self.columns = [
            replace(column, name=prefix + column.name, source=None)
            for column in columns
        ]

    def derive(self):
        state, scope = self.state, self.scope()
        column = state.choices.take(
            [column for column in self.columns if column.kind != "text"]
        )
        left = self.expression(column, scope)
        right = self.constant(column.kind, state.choices.integer(0, 8), scope)
        value = state.node(
            "pipeline.binary",
            {"left": left, "right": right},
            {"operator": state.choices.take(("add", "sub"))},
            scope=scope,
        )
        name = state.name("derived" + str(len(state.author.nodes)))
        value = state.node(
            "pipeline.named", {"value": value}, {"name": name}, scope=scope
        )
        self.stage("pipeline.derive", {"columns": [value]}, scope)
        self.columns.append(Column(name, column.kind, column.nullable))

    def nest(self):
        state = self.state
        alias = state.name("nested" + str(len(state.author.nodes)))
        source = state.node("pipeline.source", {"source": self.query}, {"alias": alias})
        self.query = state.node("pipeline.from", {"source": source})
        self.columns = [replace(column, source=alias) for column in self.columns]

    def order(self):
        state = self.state
        keys = [
            state.node(
                "pipeline.unary",
                {"value": self.expression(column)},
                {"operator": state.choices.take(("asc", "desc"))},
            )
            for column in self.columns
        ]
        self.stage("pipeline.sort", {"keys": keys})
        start = state.choices.integer(1, 3)
        self.stage(
            "pipeline.take",
            {},
            data={"start": start, "end": start + state.choices.integer(1, 5)},
        )

    def join(self):
        state = self.state
        integers = [column for column in self.columns if column.kind == "i32"]
        if not integers:
            self.nest()
            return
        alias = state.name("joined" + str(len(state.author.nodes)))
        table = state.node("table", data={"schema": "fixture", "name": "notes"})
        source = state.node("pipeline.source", {"source": table}, {"alias": alias})
        nested = state.node("pipeline.from", {"source": source})
        prefix = "j" + str(len(state.author.nodes))
        columns, expressions = [], []
        for name, kind in (("id", "i32"), ("account_id", "i32"), ("body", "text")):
            value = state.node(
                "pipeline.column", data={"source": alias, "column": name}
            )
            name = prefix + "_" + name
            expressions.append(
                state.node("pipeline.named", {"value": value}, {"name": name})
            )
            columns.append(Column(name, kind, False, alias))
        nested = state.node(
            "pipeline.select", {"query": nested, "columns": expressions}
        )
        source = state.node("pipeline.source", {"source": nested}, {"alias": alias})
        left = self.expression(state.choices.take(integers))
        right = state.node(
            "pipeline.column", data={"source": alias, "column": columns[1].name}
        )
        on = state.node(
            "pipeline.binary", {"left": left, "right": right}, {"operator": "eq"}
        )
        kind = state.choices.take(("inner", "left"))
        self.stage("pipeline.join", {"source": source, "on": on}, data={"kind": kind})
        self.columns.extend(
            replace(column, nullable=kind == "left") for column in columns
        )

    def distinct(self):
        self.stage("pipeline.distinct", {})

    def productions(self):
        count = len(self.columns)
        remaining = self.state.limits.nodes - len(self.state.author.nodes)
        costs = (
            (self.filter, 7 * 2**self.state.limits.depth),
            (self.project, 2 * count + 1),
            (self.derive, 9),
            (self.nest, 2),
            (self.order, 2 * count + 2),
            (self.join, 21),
            (self.distinct, 1),
        )
        return tuple(
            production
            for production, cost in costs
            if cost + 2 * count + 1 <= remaining
        )


def pipeline(state):
    query = Pipeline(state)
    for _ in range(state.choices.integer(2, state.limits.stages + 1)):
        available = query.productions()
        if not available:
            break
        production = state.choices.take(available)
        production()
    query.project()
    state.fetch(query.query)
