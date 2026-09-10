"""Runtime model reads and guarded writes using native SQL statements."""

from __future__ import annotations

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from .. import _native as p
from ..runtime import Connection, Pool
from ..transactions import Transaction
from .record import ModelRecord

if TYPE_CHECKING:
    from .model import Model


@dataclass(frozen=True, eq=False, slots=True)
class ModelRows:
    model: Model
    statement: p.Select | p.Insert | p.Update | p.Delete
    selected: tuple[str, ...]

    def __post_init__(self) -> None:
        from .model import Model

        if not isinstance(self.model, Model) or not isinstance(
            self.statement, (p.Select, p.Insert, p.Update, p.Delete)
        ):
            raise p.ConstructionError("model rows require a model and native statement")
        object.__setattr__(
            self, "selected", self.model._selection(tuple(self.selected))
        )

    def inspect(self) -> p.Compiled:
        return self.statement.inspect()

    async def all(self, connection: Connection | Pool | Transaction) -> list[ModelRecord]:
        return [
            ModelRecord(self.model, row, self.selected)
            for row in await connection.fetch_all(self.statement)
        ]

    async def one(self, connection: Connection | Pool | Transaction) -> ModelRecord:
        return ModelRecord(
            self.model, await connection.fetch_one(self.statement), self.selected
        )

    async def one_opt(self, connection: Connection | Pool | Transaction) -> ModelRecord | None:
        row = await connection.fetch_optional(self.statement)
        return None if row is None else ModelRecord(self.model, row, self.selected)

    def __bool__(self) -> bool:
        raise p.ConstructionError("model queries cannot be tested as Python booleans")


@dataclass(frozen=True, eq=False, slots=True)
class ModelQuery(ModelRows):
    statement: p.Select

    def __post_init__(self) -> None:
        if not isinstance(self.statement, p.Select):
            raise p.ConstructionError("model queries require a native Select")
        ModelRows.__post_init__(self)

    def filter(self, predicate: p.Expr | p.Condition) -> ModelQuery:
        return replace(self, statement=self.statement.where_(predicate))

    def order_by(self, *ordering: p.OrderBy) -> ModelQuery:
        return replace(self, statement=self.statement.order_by(*ordering))

    def limit(self, value: int | None) -> ModelQuery:
        return replace(self, statement=self.statement.limit(value))

    def offset(self, value: int | None) -> ModelQuery:
        return replace(self, statement=self.statement.offset(value))

    def join(
        self, other: Model, on: p.Expr | p.Condition, *, kind: p.Join = p.Join.Inner
    ) -> ModelQuery:
        return replace(self, statement=self.statement.join(other.table, on, kind=kind))


@dataclass(frozen=True, eq=False, slots=True)
class ModelWrite:
    model: Model
    statement: p.Insert | p.Update | p.Delete

    def __post_init__(self) -> None:
        from .model import Model

        if not isinstance(self.model, Model) or not isinstance(
            self.statement, (p.Insert, p.Update, p.Delete)
        ):
            raise p.ConstructionError(
                "model writes require a model and native CRUD statement"
            )

    def where_(self, predicate: p.Expr | p.Condition) -> ModelWrite:
        if isinstance(self.statement, p.Insert):
            raise p.UnsupportedCapabilityError(
                "model insert does not support a WHERE clause"
            )
        return replace(self, statement=self.statement.where_(predicate))

    def all_rows(self) -> ModelWrite:
        if isinstance(self.statement, p.Insert):
            raise p.UnsupportedCapabilityError(
                "model insert does not support an all-rows write guard"
            )
        return replace(self, statement=self.statement.all_rows())

    def returning(self, *fields: str) -> ModelRows:
        selected = self.model._selection(fields)
        return ModelRows(
            self.model,
            self.statement.returning(*self.model._projection(selected)),
            selected,
        )

    def inspect(self) -> p.Compiled:
        return self.statement.inspect()

    async def execute(self, connection: Connection | Pool | Transaction) -> int:
        return await connection.execute(self.statement)

    def __bool__(self) -> bool:
        raise p.ConstructionError("model writes cannot be tested as Python booleans")
