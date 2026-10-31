"""A model's writes: each value converted through its field's declared kind, a
field left out kept apart from one set to NULL, and the RETURNING list that
reads a written row's two versions as pgorm's ``exec_returning_change(s)``
and ``exec_returning_upsert(s)`` read them."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import TYPE_CHECKING, Any, cast

from .. import _native as p
from .._registered.versions import Change, Inserted, Updated
from .queries import ModelWrite
from .record import ModelRecord, Source

if TYPE_CHECKING:
    from .columns import Column
    from .model import Model

# The names a RETURNING list reads a written row's versions by: always
# renamed, as pgorm renames them, so a table called old or new cannot take
# the keyword.
OLD, NEW = "pgorm_old", "pgorm_new"

TERMINALS = {
    "returning_change": p.Update,
    "returning_changes": p.Update,
    "returning_upsert": p.Insert,
    "returning_upserts": p.Insert,
}


def assignments(
    model: Model, values: Mapping[str, Any], *, insert: bool
) -> tuple[tuple[str, p.Value | p.Expr], ...]:
    """The SQL column and value of each field ``values`` sets, in declaration
    order whatever order it names them in."""
    if not isinstance(values, Mapping):
        raise TypeError("a model write takes a mapping of the fields it sets")
    for name in values:
        field = model.col(name).field_name
        if model.columns[field].generated == "always":
            raise p.ConstructionError(f"{field} is generated always: the server writes it")
    if insert:
        for field, column in model.columns.items():
            if not column.omissible and field not in values:
                raise p.ConstructionError(
                    f"{field} is required: it is neither nullable, defaulted nor generated"
                )
    return tuple(
        (model._sql(field), _value(column, values[field]))
        for field, column in model.columns.items()
        if field in values
    )


def _value(column: Column, value: Any) -> p.Value | p.Expr:
    return value if isinstance(value, p.Expr) else column.value(value)


def inserted(model: Model, rows: Sequence[Mapping[str, Any]], *, many: bool) -> ModelWrite:
    """An INSERT of every row of ``rows``, which set the same fields."""
    if model.table.alias is not None:
        raise p.ConstructionError("model inserts require an unaliased table")
    sets = [assignments(model, row, insert=True) for row in rows]
    if not sets:
        return ModelWrite(model, None)
    columns = tuple(column for column, _ in sets[0])
    statement = p.Insert(model.table)
    if not columns:
        if len(sets) > 1:
            raise p.ConstructionError("rows that set no field are inserted one at a time")
        return ModelWrite(model, statement.default_values())
    statement = statement.columns(*columns)
    for row in sets:
        if tuple(column for column, _ in row) != columns:
            raise p.ConstructionError("every row of an insert sets the same fields")
        statement = statement.values(*(value for _, value in row))
    return ModelWrite(model, statement)


def updated(model: Model, values: Mapping[str, Any]) -> ModelWrite:
    """An UPDATE of the fields ``values`` sets, which needs a ``where`` or
    ``all_rows()`` before it runs."""
    set_ = assignments(model, values, insert=False)
    if not set_:
        raise p.ConstructionError("model update requires at least one assigned field")
    statement = p.Update(model.table)
    for column, value in set_:
        statement = statement.set(column, value)
    return ModelWrite(model, statement)


# [spec:pgorm:req:bindings.model-writes]
def versions(
    write: ModelWrite, terminal: str
) -> tuple[p.Insert | p.Update | None, tuple[Source, Source]]:
    """The statement ``terminal`` runs: the write with every field of both
    versions in a ``RETURNING WITH (OLD AS pgorm_old, NEW AS pgorm_new)``
    list, the old under ``o_`` and the new under ``n_``, and their sources."""
    kind = TERMINALS.get(terminal)
    if kind is None:
        raise TypeError(f"a model write has no terminal {terminal!r}")
    statement = write.statement
    # An insert of no rows has no statement, and is still an insert.
    if (p.Insert if statement is None else type(statement)) is not kind:
        raise p.UnsupportedCapabilityError(f"{terminal} reads a model {kind.__name__.lower()}")
    model = write.model
    fields = tuple(model.columns)
    columns = [model._sql(name) for name in fields]
    sources: list[Source] = []
    items = []
    for row, prefix in ((OLD, "o_"), (NEW, "n_")):
        names = tuple(p._model_result_names(prefix, columns))
        sources.append(Source(model, fields, names, optional=row == OLD))
        items += [p.col(column, table=row).as_(name) for column, name in zip(columns, names)]
    written = cast("p.Insert | p.Update | None", statement)
    if written is not None:
        written = written.returning(*items, old_as=OLD, new_as=NEW)
    return written, (sources[0], sources[1])


def change(pair: tuple[ModelRecord | None, ModelRecord]) -> Change[ModelRecord]:
    old, new = pair
    if old is None:
        raise p.DecodeError("an updated row returned no old version")
    return Change(old, new)


def upserted(
    pair: tuple[ModelRecord | None, ModelRecord],
) -> Inserted[ModelRecord] | Updated[ModelRecord]:
    old, new = pair
    if old is None:
        return Inserted(new)
    return Updated(Change(old, new))
