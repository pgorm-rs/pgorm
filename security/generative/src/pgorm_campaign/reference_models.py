"""Reference fixture models and graph shapes, independent of native registrations."""

from dataclasses import dataclass, replace

from .comparison import InvalidOracle
from .reference_sql import Query, Table, binary
from .reference_values import quote

ACCOUNT = (
    "id",
    "tenant",
    "name",
    "note",
    "score",
    "rank",
    "active",
    "balance",
    "payload",
    "uuid",
    "created_at",
    "occurred_at",
    "event_date",
    "event_time",
    "state",
)
NOTE = ("id", "account_id", "tenant", "body")


@dataclass(frozen=True)
class Model:
    table: Table
    fields: tuple
    entity: str | None = None

    def column(self, logical):
        return self.table.column(dict(self.fields)[logical])

    def projection(self, names=None):
        fields = (
            self.fields
            if names is None
            else [(name, dict(self.fields)[name]) for name in names]
        )
        return tuple(
            self.table.column(physical) + " AS " + quote(logical)
            for logical, physical in fields
        )

    def select(self):
        return Query(
            "select",
            columns=self.projection(),
            tables=(self.table,),
            shape={"model": self},
        )


def registered(name):
    if name not in ("campaign.Account", "campaign.Note"):
        raise InvalidOracle("independent model registration unavailable: " + name)
    columns = ACCOUNT if name == "campaign.Account" else NOTE
    return Model(
        Table("accounts" if name == "campaign.Account" else "notes", "fixture"),
        tuple((name, name) for name in columns),
        name,
    )


def descriptor(table, mapping, fixture):
    declaration = next(
        (
            item
            for item in fixture["tables"]
            if (item["schema"], item["name"]) == (table.schema, table.name)
        ),
        None,
    )
    if declaration is None:
        raise InvalidOracle("reference descriptor requires a declared fixture table")
    fields = (
        mapping
        if mapping is not None
        else {item["name"]: item["name"] for item in declaration["columns"]}
    )
    physical = {item["name"] for item in declaration["columns"]}
    if not set(fields.values()) <= physical:
        raise InvalidOracle("reference descriptor names an undeclared column")
    return Model(table, tuple(fields.items()))


def graph(name, aliases):
    arities = {
        "AccountOnly": 1,
        "OptionalNotes": 2,
        "RequiredNotes": 2,
        "SelfJoin": 2,
        **{"Arity" + str(n): n for n in range(3, 8)},
    }
    short = name.removeprefix("campaign.")
    if short not in arities or len(aliases) != arities[short] - 1:
        raise InvalidOracle("independent graph shape/alias count unsupported")
    root = registered("campaign.Account")
    models = [root]
    joins = []
    for alias in aliases:
        target = registered(
            "campaign.Account" if short == "SelfJoin" else "campaign.Note"
        )
        target = replace(target, table=replace(target.table, alias=alias))
        models.append(target)
        on = binary(
            root.column("rank" if short == "SelfJoin" else "id"),
            target.column("id" if short == "SelfJoin" else "account_id"),
            "eq",
        )
        joins.append(
            (target.table, "inner" if short == "RequiredNotes" else "left", on)
        )
    columns = tuple(
        model.table.column(physical) + " AS " + quote(f"slot_{slot}_{index}")
        for slot, model in enumerate(models)
        for index, (_, physical) in enumerate(model.fields)
    )
    return Query(
        "select",
        columns=columns,
        tables=(root.table,),
        joins=tuple(joins),
        shape={"slots": models},
    )


def reshape(records, shape):
    if "slots" in shape:
        result = []
        for row in records:
            items, offset = [], 0
            for slot, model in enumerate(shape["slots"]):
                values = row["fields"][offset : offset + len(model.fields)]
                offset += len(model.fields)
                if (slot or shape.get("optional_root")) and values[0]["value"][
                    "sql_null"
                ]:
                    items.append({"kind": "absent"})
                else:
                    items.append(
                        {
                            "kind": "record",
                            "entity": model.entity,
                            "fields": [
                                {"name": name, "value": value["value"]}
                                for (name, _), value in zip(
                                    model.fields, values, strict=True
                                )
                            ],
                        }
                    )
            result.append(
                items[0]
                if len(items) == 1 and not shape.get("tuple")
                else {"kind": "tuple", "items": items}
            )
        return result
    model = shape.get("model")
    if model is not None and model.entity is not None:
        return [
            {"kind": "record", "entity": model.entity, "fields": row["fields"]}
            for row in records
        ]
    return records


def model_node(name, i, d, fixture):
    match name:
        case "model":
            return descriptor(i["table"], d.get("fields"), fixture)
        case "entity":
            return registered(d["name"])
        case "model.column" | "entity.column":
            return i["model" if name == "model.column" else "entity"].column(d["name"])
        case "entity.predicate":
            return binary(i["entity"].column(d["column"]), i["value"], d["operator"])
        case "model.select" | "entity.find":
            model = i["model" if name == "model.select" else "entity"]
            return replace(
                model.select(),
                filters=(i["predicate"],) if "predicate" in i else (),
                order=tuple(i.get("order", ())),
            )
        case "model.write":
            model = i["model"]
            method = d["method"]
            values = tuple(i["values"])
            columns = tuple(dict(model.fields)[column] for column in d["columns"])
            query = Query(
                method,
                tables=(model.table,),
                shape={"model": model},
                filters=(i["predicate"],) if "predicate" in i else (),
            )
            if method == "insert":
                return replace(
                    query, columns=columns, records=(values,), defaults=not columns
                )
            if method == "update":
                return replace(
                    query, assignments=tuple(zip(columns, values, strict=True))
                )
            return query
        case "model.returning":
            query = i["query"]
            return replace(
                query, returning=query.shape["model"].projection(d["columns"] or None)
            )
        case "graph":
            return d["name"]
        case "graph.find":
            return graph(i["graph"], d["aliases"])
        case "graph.column":
            return i["query"].shape["slots"][d["source"]].column(d["column"])
    raise InvalidOracle("independent model semantics uncovered: " + name)
