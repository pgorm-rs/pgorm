"""Independent ActiveModel states and the documented fixture hooks."""

from dataclasses import dataclass, replace

from . import wire
from .comparison import InvalidOracle
from .reference_models import registered
from .reference_sql import Query, binary


@dataclass(frozen=True)
class Active:
    model: object
    values: dict
    states: dict


def active_node(name, i, d, results):
    match name:
        case "entity.active":
            return Active(
                i["entity"], {}, {name: "not_set" for name, _ in i["entity"].fields}
            )
        case "entity.result":
            row = results[d["step"]][d["row"]]
            if "source" in d:
                row = row["items"][d["source"]]
            if row["kind"] != "record" or "entity" not in row:
                raise InvalidOracle(
                    "independent entity result is absent or not a model"
                )
            return row
        case "entity.into_active":
            row = i["model"]
            values = {field["name"]: field["value"] for field in row["fields"]}
            return Active(
                registered(row["entity"]),
                values,
                {name: "unchanged" for name in values},
            )
        case "active.set":
            active = i["model"]
            values, states = dict(active.values), dict(active.states)
            column = d["column"]
            if column not in states:
                raise InvalidOracle("independent active state names an unknown column")
            state = d["state"]
            if state == "set":
                values[column], states[column] = i["value"], "set"
            elif state == "not_set":
                values.pop(column, None)
                states[column] = "not_set"
            elif column in values:
                states[column] = "set"
            return replace(active, values=values, states=states)
    raise InvalidOracle("independent active semantics uncovered: " + name)


def write(active, method):
    values, states, model = dict(active.values), dict(active.states), active.model
    if method not in ("insert", "update", "delete"):
        raise InvalidOracle("active writes require explicit insert, update or delete")
    if model.entity == "campaign.Account":
        if method == "insert" and "name" in values:
            values["name"] = {
                **values["name"],
                "data": values["name"]["data"] + "|hook",
            }
            states["name"] = "set"
        elif method == "update" and "rank" in values:
            values["rank"] = wire.scalar("i32", str(int(values["rank"]["data"]) + 1))
            states["rank"] = "set"
    query = Query(method, tables=(model.table,), shape={"model": model})
    if method == "insert":
        names = tuple(name for name, _ in model.fields if states[name] != "not_set")
        return replace(
            query,
            columns=names,
            records=(tuple(values[name] for name in names),),
            defaults=not names,
            returning=model.projection(),
        )
    if "id" not in values:
        raise InvalidOracle("independent active write needs a primary key")
    query = replace(query, filters=(binary(model.column("id"), values["id"], "eq"),))
    if method == "update":
        assignments = tuple(
            (name, values[name])
            for name, _ in model.fields
            if name != "id" and states[name] == "set"
        )
        if not assignments:
            # The API reads the persisted model when no field requires an update.
            return replace(model.select(), filters=query.filters)
        return replace(query, assignments=assignments, returning=model.projection())
    return query
