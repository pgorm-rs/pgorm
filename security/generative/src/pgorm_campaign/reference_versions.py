"""Independent writes that read a row's two versions, through PostgreSQL 18's RETURNING.

Each row these answer is the pair (old, new): an update's row before and after
it, and an upsert's, the old one absent for a row the insert wrote. They are
statement terminals, so no ActiveModel hook is applied here either.
"""

from .comparison import InvalidOracle
from .reference_models import key
from .reference_sql import SQL, Query, binary
from .reference_values import quote


def _shape(model):
    return {"slots": [model, model], "optional_root": True, "tuple": True}


def _both(model):
    """Every column of the written row, before and after, as the reshape reads it."""
    return tuple(
        SQL((version + "." + quote(physical) + " AS " + quote(f"slot_{slot}_{index}"),))
        for slot, version in enumerate(("old", "new"))
        for index, (_, physical) in enumerate(model.fields)
    )


def change(active):
    """`UpdateOne::exec_returning_change`: by every key column; with nothing
    set, nothing is sent and the row the key reads is both versions."""
    model, values, states = active.model, active.values, active.states
    keys = key(model.entity)
    if any(name not in values for name in keys):
        raise InvalidOracle("independent versioned update needs a primary key")
    filters = tuple(binary(model.column(name), values[name], "eq") for name in keys)
    assignments = tuple(
        (name, values[name])
        for name, _ in model.fields
        if name not in keys and states[name] == "set"
    )
    if not assignments:
        columns = tuple(
            model.table.column(physical) + " AS " + quote(f"slot_{slot}_{index}")
            for slot in (0, 1)
            for index, (_, physical) in enumerate(model.fields)
        )
        return Query(
            "select",
            columns=columns,
            tables=(model.table,),
            filters=filters,
            shape=_shape(model),
        )
    return Query(
        "update",
        tables=(model.table,),
        filters=filters,
        assignments=assignments,
        returning=_both(model),
        shape=_shape(model),
    )


def changes(model, columns, values, predicate):
    """`UpdateMany::exec_returning_changes` over the rows `predicate` admits."""
    if not columns:
        raise InvalidOracle("independent versioned update sets no column")
    return Query(
        "update",
        tables=(model.table,),
        filters=(predicate,),
        assignments=tuple(
            (dict(model.fields)[column], value)
            for column, value in zip(columns, values, strict=True)
        ),
        returning=_both(model),
        shape=_shape(model),
    )


def upsert(actives, data):
    """`Insert::exec_returning_upsert(s)`: every model's set columns, which an
    insert of many holds uniform, and the declared conflict action."""
    model = actives[0].model
    if any(active.model.entity != model.entity for active in actives):
        raise InvalidOracle("independent upsert mixes registrations")
    names = tuple(
        name for name, _ in model.fields if actives[0].states[name] != "not_set"
    )
    for active in actives:
        if tuple(n for n, _ in model.fields if active.states[n] != "not_set") != names:
            raise InvalidOracle("independent upsert batch sets different columns")
    return Query(
        "insert",
        tables=(model.table,),
        columns=names,
        records=tuple(
            tuple(active.values[name] for name in names) for active in actives
        ),
        conflict={
            "keys": tuple(data["conflict"]),
            "action": "update" if data["update"] else "nothing",
            "columns": tuple(data["update"]),
        },
        returning=_both(model),
        shape=_shape(model),
    )
