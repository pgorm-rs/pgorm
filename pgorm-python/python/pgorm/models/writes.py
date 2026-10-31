"""A model's writes: each value converted through its field's declared kind, a
field left out kept apart from one set to NULL, and the RETURNING list that
reads a written row's two versions as pgorm's ``exec_returning_change(s)``
and ``exec_returning_upsert(s)`` read them.

An insert, an update and a delete are each a class of its own holding only
the methods its statement takes, so a type checker refuses an insert's
``where`` or a delete's ``returning_change`` as it refuses any attribute a
class lacks."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING, Any, ClassVar, Literal, Self

from .. import _native as p
from .._registered.versions import Change, Inserted, Updated
from .queries import ModelRows, predicate_of, written
from .record import Database, ModelRecord, Source, decode_all, decode_single

if TYPE_CHECKING:
    from .columns import Column
    from .model import Model

# The names a RETURNING list reads a written row's versions by: always
# renamed, as pgorm renames them, so a table called old or new cannot take
# the keyword.
OLD, NEW = "pgorm_old", "pgorm_new"


@dataclass(frozen=True, eq=False, slots=True)
class _Write[S: p.Insert | p.Update | p.Delete | None]:
    """What every model write holds: its model and its native statement,
    which ``statement`` exposes, and the terminals every write has. Only an
    insert of no rows has no statement, and it sends nothing."""

    model: Model
    statement: S
    _statements: ClassVar[tuple[type, ...]]
    _made_by: ClassVar[str]

    def __post_init__(self) -> None:
        from .model import Model

        if not isinstance(self.model, Model) or not isinstance(
            self.statement, self._statements
        ):
            raise TypeError(f"a {type(self).__name__} comes from a model's {self._made_by}")

    def returning(self, *fields: str) -> ModelRows:
        """A RETURNING list of ``fields``, every field when it names none,
        whose terminals decode each written row as a record."""
        model = self.model
        selected = model._selection(fields)
        if self.statement is None:
            return ModelRows(model, None, selected)
        columns = (model._projected(name, p.col(model._sql(name))) for name in selected)
        return ModelRows(model, self.statement.returning(*columns), selected)

    async def execute(self, db: Database) -> int:
        """How many rows the write wrote."""
        if self.statement is None:
            return 0
        return await db.execute(self.statement)

    def __bool__(self) -> bool:
        raise p.ConstructionError("model writes cannot be tested as Python booleans")


@dataclass(frozen=True, eq=False, slots=True)
class _Guarded[S: (p.Update, p.Delete)](_Write[S]):
    """A write of the rows a condition picks, which needs ``where`` or
    ``all_rows()`` before it runs, as the statement builders' writes do."""

    def where(self, predicate: p.Expr | p.Condition) -> Self:
        return replace(self, statement=self.statement.where(predicate_of(predicate)))

    def all_rows(self) -> Self:
        return replace(self, statement=self.statement.all_rows())

    def inspect(self) -> p.Compiled:
        """The SQL and values the write runs."""
        return self.statement.inspect()


# [spec:pgorm:req:bindings.model-writes+1]
@dataclass(frozen=True, eq=False, slots=True)
class ModelInsert(_Write[p.Insert | None]):
    """A model's INSERT, made by its ``insert()`` or ``insert_many()``."""

    _statements = (p.Insert, type(None))
    _made_by = "insert() or insert_many()"

    def on_conflict(self, action: p.Conflict | p.ConflictUpdate) -> ModelInsert:
        """What a conflict does, the statement builders' ``Conflict`` naming
        SQL columns."""
        if not isinstance(action, (p.Conflict, p.ConflictUpdate)):
            raise TypeError(
                "a conflict's action is Conflict.do_nothing(), or a target's do_nothing() or update(..)"
            )
        return replace(self, statement=written(self.statement).on_conflict(action))

    def inspect(
        self, *, terminal: Literal["returning_upsert", "returning_upserts"] | None = None
    ) -> p.Compiled:
        """The SQL and values the insert runs, or the ones its upsert
        terminals run."""
        if terminal is None:
            return written(self.statement).inspect()
        if terminal not in ("returning_upsert", "returning_upserts"):
            raise TypeError(
                'an insert inspects itself, or its "returning_upsert" or "returning_upserts"'
            )
        return written(self._upserts()[0]).inspect()

    async def returning_upsert(
        self, db: Database
    ) -> Inserted[ModelRecord] | Updated[ModelRecord] | None:
        """What the insert did with its one row, or None where its conflict
        clause wrote nothing."""
        statement, sources = self._upserts()
        if statement is None:
            return None
        pair = await decode_single(db, statement, sources, optional=True)
        return None if pair is None else upserted(pair)

    async def returning_upserts(
        self, db: Database
    ) -> list[Inserted[ModelRecord] | Updated[ModelRecord]]:
        """What the insert did with each row it wrote; a row its conflict
        clause held back is left out."""
        statement, sources = self._upserts()
        if statement is None:
            return []
        return [upserted(pair) for pair in await decode_all(db, statement, sources)]

    def _upserts(self) -> tuple[p.Insert | None, tuple[Source, Source]]:
        items, sources = versions(self.model)
        if self.statement is None:
            return None, sources
        return self.statement.returning(*items, old_as=OLD, new_as=NEW), sources


# [spec:pgorm:req:bindings.model-writes+1]
@dataclass(frozen=True, eq=False, slots=True)
class ModelUpdate(_Guarded[p.Update]):
    """A model's UPDATE, made by its ``update()``."""

    _statements = (p.Update,)
    _made_by = "update()"

    def inspect(
        self, *, terminal: Literal["returning_change", "returning_changes"] | None = None
    ) -> p.Compiled:
        """The SQL and values the update runs, or the ones its change
        terminals run."""
        if terminal is None:
            return self.statement.inspect()
        if terminal not in ("returning_change", "returning_changes"):
            raise TypeError(
                'an update inspects itself, or its "returning_change" or "returning_changes"'
            )
        return self._changes()[0].inspect()

    async def returning_change(self, db: Database) -> Change[ModelRecord]:
        """The one row the update wrote, before and after; any other count
        is a ``DecodeError``."""
        statement, sources = self._changes()
        return change(await decode_single(db, statement, sources, optional=False))

    async def returning_changes(self, db: Database) -> list[Change[ModelRecord]]:
        """Every row the update wrote, before and after."""
        statement, sources = self._changes()
        return [change(pair) for pair in await decode_all(db, statement, sources)]

    def _changes(self) -> tuple[p.Update, tuple[Source, Source]]:
        items, sources = versions(self.model)
        return self.statement.returning(*items, old_as=OLD, new_as=NEW), sources


# [spec:pgorm:req:bindings.model-writes+1]
@dataclass(frozen=True, eq=False, slots=True)
class ModelDelete(_Guarded[p.Delete]):
    """A model's DELETE, made by its ``delete()``."""

    _statements = (p.Delete,)
    _made_by = "delete()"


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
    return value if isinstance(value, p.Expr) else column._value(value)


def inserted(model: Model, rows: Sequence[Mapping[str, Any]]) -> ModelInsert:
    """An INSERT of every row of ``rows``, which set the same fields."""
    if model.table.alias is not None:
        raise p.ConstructionError("model inserts require an unaliased table")
    sets = [assignments(model, row, insert=True) for row in rows]
    if not sets:
        return ModelInsert(model, None)
    columns = tuple(column for column, _ in sets[0])
    statement = p.Insert(model.table)
    if not columns:
        if len(sets) > 1:
            raise p.ConstructionError("rows that set no field are inserted one at a time")
        return ModelInsert(model, statement.default_values())
    statement = statement.columns(*columns)
    for row in sets:
        if tuple(column for column, _ in row) != columns:
            raise p.ConstructionError("every row of an insert sets the same fields")
        statement = statement.values(*(value for _, value in row))
    return ModelInsert(model, statement)


def updated(model: Model, values: Mapping[str, Any]) -> ModelUpdate:
    """An UPDATE of the fields ``values`` sets, which needs a ``where`` or
    ``all_rows()`` before it runs."""
    set_ = assignments(model, values, insert=False)
    if not set_:
        raise p.ConstructionError("model update requires at least one assigned field")
    statement = p.Update(model.table)
    for column, value in set_:
        statement = statement.set(column, value)
    return ModelUpdate(model, statement)


# [spec:pgorm:req:bindings.model-writes+1]
def versions(model: Model) -> tuple[list[p.Aliased], tuple[Source, Source]]:
    """Every field of both versions for a ``RETURNING WITH (OLD AS
    pgorm_old, NEW AS pgorm_new)`` list, the old under ``o_`` and the new
    under ``n_``, and the sources each decodes as."""
    fields = tuple(model.columns)
    columns = [model._sql(name) for name in fields]
    sources: list[Source] = []
    items = []
    for row, prefix in ((OLD, "o_"), (NEW, "n_")):
        names = tuple(p._model_result_names(prefix, columns))
        sources.append(Source(model, fields, names, optional=row == OLD))
        items += [p.col(column, table=row).as_(name) for column, name in zip(columns, names)]
    return items, (sources[0], sources[1])


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
