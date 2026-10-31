"""Runtime model reads and guarded writes using native SQL statements."""

from __future__ import annotations

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING, cast

from .. import _native as p
from .record import (
    Database,
    ModelRecord,
    Source,
    decode_all,
    decode_count,
    decode_single,
    decode_stream,
    own_source,
)

if TYPE_CHECKING:
    from ..results import RowStream
    from ..runtime import Connection, Pool
    from .cursors import Cursor, Paginator
    from .model import Model
    from .relations import Relation
    from .._registered.versions import Change, Inserted, Updated


def predicate_of(predicate: object) -> p.Expr | p.Condition:
    if not isinstance(predicate, (p.Expr, p.Condition)):
        raise TypeError("a condition is an expression or a Condition")
    return predicate


def orderings_of(orderings: tuple[object, ...]) -> tuple[p.OrderBy, ...]:
    checked = tuple(ordering for ordering in orderings if isinstance(ordering, p.OrderBy))
    if len(checked) != len(orderings):
        raise TypeError("an ordering is expr.asc() or expr.desc()")
    return checked


@dataclass(frozen=True, slots=True)
class Joined:
    """A table a model query joins without reading its columns."""

    model: Model
    table: p.Table
    qualifier: str
    on: p.Condition
    kind: p.Join


@dataclass(frozen=True, slots=True)
class Recipe:
    """What a model query reads: its fields, joins, conditions, ordering and
    window, from which each terminal's statement is built."""

    model: Model
    fields: tuple[str, ...]
    joins: tuple[Joined, ...] = ()
    wheres: tuple[p.Expr | p.Condition, ...] = ()
    orders: tuple[p.OrderBy, ...] = ()
    limit: int | None = None
    offset: int | None = None

    def select(
        self,
        orders: tuple[p.OrderBy, ...] | None = None,
        wheres: tuple[p.Expr | p.Condition, ...] = (),
        limit: int | None = None,
    ) -> p.Select:
        """The SELECT, its ordering, further conditions and limit those given."""
        model = self.model
        query = p.Select(
            *(model._projected(name, model.col(name)) for name in self.fields)
        ).from_(model.table)
        for join in self.joins:
            query = query.join(join.table, join.on, kind=join.kind)
        for where in (*self.wheres, *wheres):
            query = query.where(where)
        ordering = self.orders if orders is None else orders
        if ordering:
            query = query.order_by(*ordering)
        return query.limit(limit).offset(self.offset)


# [spec:pgorm:req:bindings.model-reads]
@dataclass(frozen=True, eq=False, slots=True)
class ModelQuery:
    """A model's SELECT: every method returns a new query, and the terminals
    decode each row into a record of the fields it selects."""

    recipe: Recipe

    def __post_init__(self) -> None:
        if not isinstance(self.recipe, Recipe):
            raise TypeError("a ModelQuery comes from a model's find() or select()")
        # Building the statement refuses what cannot be built as the query is made.
        self.statement

    @property
    def model(self) -> Model:
        return self.recipe.model

    @property
    def selected(self) -> tuple[str, ...]:
        return self.recipe.fields

    @property
    def statement(self) -> p.Select:
        return self.recipe.select(limit=self.recipe.limit)

    def inspect(self, *, terminal: str | None = None) -> p.Compiled:
        """The SQL and values the query runs, or with ``terminal="count"``
        the ones ``count`` runs."""
        if terminal is None:
            return self.statement.inspect()
        if terminal == "count":
            return p._model_count(self.statement).inspect()
        raise ValueError('a model query inspects itself, or its "count"')

    def where(self, predicate: p.Expr | p.Condition) -> ModelQuery:
        recipe = self.recipe
        return ModelQuery(replace(recipe, wheres=(*recipe.wheres, predicate_of(predicate))))

    def order_by(self, *ordering: p.OrderBy) -> ModelQuery:
        recipe = self.recipe
        return ModelQuery(replace(recipe, orders=(*recipe.orders, *orderings_of(ordering))))

    def limit(self, value: int | None) -> ModelQuery:
        return ModelQuery(replace(self.recipe, limit=value))

    def offset(self, value: int | None) -> ModelQuery:
        return ModelQuery(replace(self.recipe, offset=value))

    def join(
        self,
        relation: Relation,
        *,
        kind: p.Join = p.Join.Inner,
        alias: str | None = None,
    ) -> ModelQuery:
        """Join the table at the far end of ``relation``, from the query's
        model or a table it joined, without reading its columns."""
        from .relations import Relation, joined_on, same_table

        if not isinstance(relation, Relation):
            raise TypeError("a model query joins a Relation")
        if kind != p.Join.Inner and kind != p.Join.Left:
            raise ValueError("a model join is Join.Inner or Join.Left")
        recipe = self.recipe
        tables = [(recipe.model, recipe.model.qualifier)]
        tables += [(join.model, join.qualifier) for join in recipe.joins]
        start = next((q for model, q in tables if same_table(model, relation.from_)), None)
        if start is None:
            raise p.ConstructionError(
                f"the relation starts at {relation.from_.table.name}, which the query does not read"
            )
        end = relation.to
        table = end.table if alias is None else end.table.as_(alias)
        qualifier = end.qualifier if alias is None else p.Identifier(alias).name
        on = joined_on(relation, start, qualifier)
        join = Joined(end, table, qualifier, on, kind)
        return ModelQuery(replace(recipe, joins=(*recipe.joins, join)))

    async def all(self, db: Database) -> list[ModelRecord]:
        return await decode_all(db, self.statement, self._sources())

    async def one(self, db: Database) -> ModelRecord:
        return cast(ModelRecord, await decode_single(db, self.statement, self._sources(), optional=False))

    async def optional(self, db: Database) -> ModelRecord | None:
        return cast(ModelRecord | None, await decode_single(db, self.statement, self._sources(), optional=True))

    async def stream(self, db: Connection | Pool) -> RowStream[ModelRecord]:
        """The rows, one per pull, over a connection the stream holds until
        it ends."""
        return await decode_stream(db, self.statement, self._sources())

    async def count(self, db: Database) -> int:
        """How many rows the query reads, its limit and offset aside."""
        return await decode_count(db, self.statement)

    def cursor(self, *fields: str) -> Cursor:
        """A keyset cursor over the rows, ordered by ``fields``."""
        from .cursors import Cursor, Keyed

        recipe = self.recipe
        if not fields:
            raise p.ConstructionError("a cursor orders by at least one field")
        qualifier = recipe.model.qualifier
        order = tuple((recipe.model, qualifier, recipe.model.col(name).field_name) for name in fields)
        return Cursor(
            Keyed(
                lambda orders, wheres, limit: recipe.select(orders, wheres, limit),
                recipe.limit,
                self._sources(),
                order,
                (),
            )
        )

    def paginate(self, page_size: int) -> Paginator:
        """Pages of ``page_size`` rows."""
        from .cursors import Pageable, Paginator

        recipe = self.recipe
        return Paginator(
            Pageable(
                lambda limit, offset: replace(recipe, offset=offset).select(limit=limit),
                p._model_count(self.statement),
                self._sources(),
            ),
            page_size,
        )

    def _sources(self) -> tuple[Source, ...]:
        return (own_source(self.recipe.model, self.recipe.fields),)

    def __bool__(self) -> bool:
        raise p.ConstructionError("model queries cannot be tested as Python booleans")


@dataclass(frozen=True, eq=False, slots=True)
class ModelRows:
    """A write with a RETURNING list of a model's fields, whose terminals
    decode each returned row; an insert of no rows has no statement and
    returns none."""

    model: Model
    statement: p.Insert | p.Update | p.Delete | None
    selected: tuple[str, ...]

    def __post_init__(self) -> None:
        from .model import Model

        if not isinstance(self.model, Model) or not isinstance(
            self.statement, (p.Insert, p.Update, p.Delete, type(None))
        ):
            raise TypeError("model rows are a model's native write statement")
        object.__setattr__(
            self, "selected", self.model._selection(tuple(self.selected))
        )

    def inspect(self) -> p.Compiled:
        return _written(self.statement).inspect()

    async def all(self, db: Database) -> list[ModelRecord]:
        if self.statement is None:
            return []
        return await decode_all(db, self.statement, self._sources())

    async def one(self, db: Database) -> ModelRecord:
        if self.statement is None:
            raise p.DecodeError("expected exactly one row, and the insert wrote none")
        return cast(ModelRecord, await decode_single(db, self.statement, self._sources(), optional=False))

    async def optional(self, db: Database) -> ModelRecord | None:
        if self.statement is None:
            return None
        return cast(ModelRecord | None, await decode_single(db, self.statement, self._sources(), optional=True))

    def _sources(self) -> tuple[Source, ...]:
        return (own_source(self.model, self.selected),)

    def __bool__(self) -> bool:
        raise p.ConstructionError("model queries cannot be tested as Python booleans")


def _written(
    statement: p.Insert | p.Update | p.Delete | None,
) -> p.Insert | p.Update | p.Delete:
    if statement is None:
        raise p.ConstructionError("an insert of no rows writes nothing and has no statement")
    return statement


# [spec:pgorm:req:bindings.model-writes]
@dataclass(frozen=True, eq=False, slots=True)
class ModelWrite:
    """A model's INSERT, UPDATE or DELETE; an insert of no rows has no
    statement and sends nothing."""

    model: Model
    statement: p.Insert | p.Update | p.Delete | None

    def __post_init__(self) -> None:
        from .model import Model

        if not isinstance(self.model, Model) or not isinstance(
            self.statement, (p.Insert, p.Update, p.Delete, type(None))
        ):
            raise TypeError("a model write is a model's native write statement")

    def where(self, predicate: p.Expr | p.Condition) -> ModelWrite:
        if isinstance(self.statement, (p.Insert, type(None))):
            raise p.UnsupportedCapabilityError(
                "model insert does not support a WHERE clause"
            )
        return replace(self, statement=self.statement.where(predicate_of(predicate)))

    def all_rows(self) -> ModelWrite:
        if isinstance(self.statement, (p.Insert, type(None))):
            raise p.UnsupportedCapabilityError(
                "model insert does not support an all-rows write guard"
            )
        return replace(self, statement=self.statement.all_rows())

    def on_conflict(self, action: p.Conflict | p.ConflictUpdate) -> ModelWrite:
        """What an insert's conflict does, the statement builders' ``Conflict``
        naming SQL columns."""
        if not isinstance(action, (p.Conflict, p.ConflictUpdate)):
            raise TypeError(
                "a conflict's action is Conflict.do_nothing(), or a target's do_nothing() or update(..)"
            )
        statement = _written(self.statement)
        if not isinstance(statement, p.Insert):
            raise p.UnsupportedCapabilityError("only a model insert has a conflict clause")
        return replace(self, statement=statement.on_conflict(action))

    def returning(self, *fields: str) -> ModelRows:
        selected = self.model._selection(fields)
        if self.statement is None:
            return ModelRows(self.model, None, selected)
        columns = (self.model._projected(name, p.col(self.model._sql(name))) for name in selected)
        return ModelRows(self.model, self.statement.returning(*columns), selected)

    def inspect(self, *, terminal: str | None = None) -> p.Compiled:
        """The SQL and values the write runs, or the ones a terminal reading
        each written row's two versions runs."""
        from .writes import versions

        if terminal is None:
            return _written(self.statement).inspect()
        return _written(versions(self, terminal)[0]).inspect()

    async def execute(self, db: Database) -> int:
        if self.statement is None:
            return 0
        return await db.execute(self.statement)

    async def returning_change(self, db: Database) -> Change[ModelRecord]:
        """The one row the update wrote, before and after."""
        from .writes import change, versions

        statement, sources = versions(self, "returning_change")
        return change(await decode_single(db, statement, sources, optional=False))

    async def returning_changes(self, db: Database) -> list[Change[ModelRecord]]:
        """Every row the update wrote, before and after."""
        from .writes import change, versions

        statement, sources = versions(self, "returning_changes")
        return [change(pair) for pair in await decode_all(db, statement, sources)]

    async def returning_upsert(
        self, db: Database
    ) -> Inserted[ModelRecord] | Updated[ModelRecord] | None:
        """What the insert did with its one row, or None where its conflict
        clause wrote nothing."""
        from .writes import upserted, versions

        statement, sources = versions(self, "returning_upsert")
        if statement is None:
            return None
        pair = await decode_single(db, statement, sources, optional=True)
        return None if pair is None else upserted(pair)

    async def returning_upserts(
        self, db: Database
    ) -> list[Inserted[ModelRecord] | Updated[ModelRecord]]:
        """What the insert did with each row it wrote; a row its conflict
        clause held back is left out."""
        from .writes import upserted, versions

        statement, sources = versions(self, "returning_upserts")
        if statement is None:
            return []
        return [upserted(pair) for pair in await decode_all(db, statement, sources)]

    def __bool__(self) -> bool:
        raise p.ConstructionError("model writes cannot be tested as Python booleans")
