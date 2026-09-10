"""Small independent SQL algebra: names are quoted and every value is bound."""

from dataclasses import dataclass, field, replace

from .comparison import InvalidOracle
from .reference_values import argument, qualified, quote, sql_type


@dataclass(frozen=True)
class Parameter:
    value: dict


@dataclass(frozen=True)
class SQL:
    parts: tuple = ()

    def __add__(self, other):
        return SQL(self.parts + (other.parts if isinstance(other, SQL) else (other,)))

    def __radd__(self, other):
        return SQL((other,)) + self

    def command(self):
        text, parameters = [], []
        for part in self.parts:
            if isinstance(part, Parameter):
                text.append("%s")
                parameters.append(argument(part.value))
            elif isinstance(part, str):
                text.append(part.replace("%", "%%"))
            else:
                raise InvalidOracle("invalid independent SQL fragment")
        return "".join(text), parameters


def join(items, separator=", "):
    result = SQL()
    for index, item in enumerate(items):
        result += separator if index else ""
        result += item
    return result


def bound(value):
    tag = value["type"]
    if tag["kind"] == "array" and not value["sql_null"]:
        return (
            "ARRAY["
            + join([bound(item) for item in value["data"]])
            + "]::"
            + sql_type(tag)
        )
    return SQL((Parameter(value), "::" + sql_type(tag)))


def expression(value):
    return bound(value) if isinstance(value, dict) and "sql_null" in value else value


def binary(left, right, operator):
    symbols = {
        "eq": "=",
        "ne": "<>",
        "lt": "<",
        "lte": "<=",
        "gt": ">",
        "gte": ">=",
        "add": "+",
        "sub": "-",
        "mul": "*",
        "div": "/",
        "mod": "%",
        "and": "AND",
        "or": "OR",
    }
    return (
        "(" + expression(left) + " " + symbols[operator] + " " + expression(right) + ")"
    )


def condition(items, mode, negated=False):
    result = (
        join(items, " AND " if mode == "all" else " OR ")
        if items
        else SQL(("TRUE" if mode == "all" else "FALSE",))
    )
    return ("NOT (" if negated else "(") + result + ")"


@dataclass(frozen=True)
class Table:
    name: str
    schema: str | None = None
    alias: str | None = None

    def source(self):
        return qualified(self.name, self.schema) + (
            " AS " + quote(self.alias) if self.alias else ""
        )

    def column(self, name):
        return SQL((qualified(name, self.alias or self.name),))


@dataclass(frozen=True)
class Query:
    kind: str
    columns: tuple = ()
    tables: tuple = ()
    filters: tuple = ()
    joins: tuple = ()
    group: tuple = ()
    having: tuple = ()
    order: tuple = ()
    limit: int | None = None
    offset: int | None = None
    distinct: bool = False
    records: tuple = ()
    defaults: bool = False
    assignments: tuple = ()
    conflict: dict = field(default_factory=dict)
    returning: tuple = ()
    shape: dict = field(default_factory=dict)

    def select(self):
        if not self.columns:
            raise InvalidOracle("reference SELECT needs explicit projections")
        result = "SELECT " + ("DISTINCT " if self.distinct else "") + join(self.columns)
        if self.tables:
            result += " FROM " + join([table.source() for table in self.tables])
        for table, kind, on in self.joins:
            result += " " + kind.upper() + " JOIN " + table.source()
            if kind != "cross":
                result += " ON " + on
        return result

    def write(self):
        if len(self.tables) != 1:
            raise InvalidOracle("reference write needs one target table")
        table = self.tables[0].source()
        if self.kind == "delete":
            return SQL(("DELETE FROM " + table,))
        if self.kind == "update":
            if not self.assignments:
                raise InvalidOracle("reference UPDATE has no assignments")
            return (
                "UPDATE "
                + table
                + " SET "
                + join(
                    [
                        quote(key) + " = " + expression(value)
                        for key, value in self.assignments
                    ]
                )
            )
        if self.kind != "insert":
            raise InvalidOracle("unknown reference query category")
        result = SQL(("INSERT INTO " + table,))
        if self.defaults:
            result += " DEFAULT VALUES"
        elif self.records and self.columns:
            result += " (" + join([quote(name) for name in self.columns]) + ") VALUES "
            result += join(
                [
                    "(" + join([expression(value) for value in row]) + ")"
                    for row in self.records
                ]
            )
        else:
            raise InvalidOracle("empty insert needs explicit default semantics")
        if self.conflict:
            result += (
                " ON CONFLICT ("
                + join([quote(name) for name in self.conflict["keys"]])
                + ") DO "
            )
            if self.conflict["action"] == "nothing":
                result += "NOTHING"
            else:
                result += "UPDATE SET " + join(
                    [
                        SQL((quote(name) + " = " + qualified(name, "excluded"),))
                        for name in self.conflict["columns"]
                    ]
                )
        return result

    def sql(self):
        result = self.select() if self.kind == "select" else self.write()
        for prefix, values, separator in (
            (" WHERE ", self.filters, " AND "),
            (" GROUP BY ", self.group, ", "),
            (" HAVING ", self.having, " AND "),
            (" ORDER BY ", self.order, ", "),
        ):
            if values:
                result += prefix + join(values, separator)
        if self.limit is not None:
            result += " LIMIT " + str(self.limit)
        if self.offset is not None:
            result += " OFFSET " + str(self.offset)
        if self.returning:
            result += " RETURNING " + join(self.returning)
        return result


def query_node(name, inputs, data):
    query = inputs.get("query")
    match name:
        case "select":
            return Query("select", columns=tuple(inputs["columns"]))
        case "insert" | "update" | "delete":
            return Query(
                name, tables=(inputs["table"],), columns=tuple(data.get("columns", ()))
            )
        case "select.from":
            return replace(query, tables=(*query.tables, inputs["table"]))
        case "select.filter" | "write.filter" | "entity.filter" | "graph.filter":
            return replace(query, filters=(*query.filters, inputs["predicate"]))
        case "select.join":
            return replace(
                query,
                joins=(*query.joins, (inputs["table"], data["kind"], inputs.get("on"))),
            )
        case "select.group":
            return replace(query, group=(*query.group, *inputs["keys"]))
        case "select.having":
            return replace(query, having=(*query.having, inputs["predicate"]))
        case "select.order" | "entity.order" | "graph.order":
            return replace(query, order=(*query.order, *inputs["keys"]))
        case "select.page" | "entity.page":
            return replace(query, **data)
        case "select.distinct":
            return replace(query, distinct=True)
        case "insert.row":
            return replace(query, records=(*query.records, tuple(inputs["values"])))
        case "insert.defaults":
            return replace(query, defaults=True)
        case "insert.conflict":
            return replace(query, conflict=data)
        case "update.set":
            return replace(
                query,
                assignments=(*query.assignments, (data["column"], inputs["value"])),
            )
        case "write.all":
            return query
        case "write.returning":
            return replace(query, returning=tuple(inputs["columns"]))
    raise InvalidOracle("independent query semantics uncovered: " + name)
