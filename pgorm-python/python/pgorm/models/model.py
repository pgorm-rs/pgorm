"""Application-defined model metadata, lowered through native statement builders."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass, replace
from types import MappingProxyType
from typing import TYPE_CHECKING, Any, cast

from .. import _native as p
from .columns import Column, ModelColumn

if TYPE_CHECKING:
    from .graphs import Graph
    from .queries import ModelQuery
    from .relations import Relation
    from .writes import ModelDelete, ModelInsert, ModelUpdate


# [spec:pgorm:req:bindings.models+3]
@dataclass(frozen=True, eq=False, slots=True, init=False)
class Model:
    table: p.Table
    columns: Mapping[str, Column]
    primary_key: tuple[str, ...]

    def __init__(
        self,
        table: str | p.Table,
        columns: Mapping[str, Column],
        *,
        schema: str | None = None,
    ) -> None:
        if isinstance(table, p.Table):
            if schema is not None:
                raise p.ConstructionError(
                    "schema is already part of the supplied Table"
                )
            native = table
        else:
            native = p.Table(table, schema=schema)
        if not isinstance(columns, Mapping):
            raise TypeError("a model's columns are a mapping of fields to Columns")
        if not columns:
            raise p.ConstructionError("model columns require a non-empty mapping")
        declarations = {}
        physical = set()
        for identity, column in columns.items():
            identity = p.Identifier(identity).name
            if identity in declarations:
                raise p.ConstructionError("duplicate model field identity")
            if not isinstance(column, Column):
                raise TypeError("each model field is a Column declaration")
            column = replace(
                column, name=identity if column.name is None else column.name
            )
            if column.name in physical:
                raise p.ConstructionError(
                    "multiple model fields map to the same SQL column"
                )
            declarations[identity] = column
            physical.add(column.name)
        object.__setattr__(self, "table", native)
        object.__setattr__(self, "columns", MappingProxyType(declarations))
        object.__setattr__(
            self,
            "primary_key",
            tuple(name for name, column in declarations.items() if column.primary_key),
        )

    @property
    def qualifier(self) -> str:
        """What the model's columns are qualified by: its alias, or else its
        table's name, never its schema, as pgorm qualifies an entity's."""
        return self.table.alias or self.table.name

    def as_(self, alias: str) -> Model:
        return Model(self.table.as_(alias), self.columns)

    def col(self, field: str) -> ModelColumn:
        return self._column(field, self.qualifier)

    def _column(self, field: str, qualifier: str) -> ModelColumn:
        name = p.Identifier(field).name
        if name not in self.columns:
            raise p.ConstructionError(f"{self.table.name} has no field {name!r}")
        return ModelColumn(name, self.columns[name], p.col(self._sql(name), table=qualifier))

    def _sql(self, field: str) -> str:
        return cast(str, self.columns[field].name)

    def _projected(self, field: str, column: p.Expr) -> p.Expr | p.Aliased:
        """``column`` projected under the field's name: as it is when the
        field is its column's name, aliased otherwise."""
        return column if field == self._sql(field) else column.as_(field)

    def _selection(self, fields: tuple[str, ...]) -> tuple[str, ...]:
        selected = tuple(self.columns) if not fields else fields
        names = tuple(self.col(name).field_name for name in selected)
        if len(set(names)) != len(names):
            raise p.ConstructionError("duplicate model output identity")
        return names

    def select(self, *fields: str) -> ModelQuery:
        from .queries import ModelQuery, Recipe

        return ModelQuery(Recipe(self, self._selection(fields)))

    def find(self) -> ModelQuery:
        return self.select()

    def find_by_key(self, values: Mapping[str, Any]) -> ModelQuery:
        """The row whose primary key ``values`` gives."""
        return self.find().where(self.key(values))

    def key(self, values: Mapping[str, Any]) -> p.Condition:
        if not self.primary_key:
            raise p.ConstructionError("model has no declared primary key")
        if not isinstance(values, Mapping):
            raise TypeError("a key is a mapping of the key's fields")
        if set(values) != set(self.primary_key):
            raise p.ConstructionError(
                "key lookup requires every declared primary-key field and no others"
            )
        return p.Condition.all(
            *(self.col(name).eq(values[name]) for name in self.primary_key)
        )

    def insert(self, values: Mapping[str, Any]) -> ModelInsert:
        from .writes import inserted

        return inserted(self, [values])

    def insert_many(self, rows: Sequence[Mapping[str, Any]]) -> ModelInsert:
        """An INSERT of rows that each set the same fields; no rows writes
        nothing and sends nothing."""
        from .writes import inserted

        if not isinstance(rows, (list, tuple)):
            raise TypeError("insert_many takes a list or tuple of rows")
        return inserted(self, rows)

    def update(self, values: Mapping[str, Any]) -> ModelUpdate:
        from .writes import updated

        return updated(self, values)

    def delete(self) -> ModelDelete:
        """A DELETE, which needs a ``where`` or ``all_rows()`` before it
        runs."""
        from .writes import ModelDelete

        return ModelDelete(self, p.Delete(self.table))

    def belongs_to(
        self, target: Model, *, from_: str | Sequence[str], to: str | Sequence[str]
    ) -> Relation:
        """A relation from this model's foreign key to the key of ``target``."""
        from .relations import Relation

        return Relation("belongs_to", self, target, from_, to)

    def has_one(
        self, target: Model, *, from_: str | Sequence[str], to: str | Sequence[str]
    ) -> Relation:
        """A relation from this model's key to at most one row of ``target``."""
        from .relations import Relation

        return Relation("has_one", self, target, from_, to)

    def has_many(
        self, target: Model, *, from_: str | Sequence[str], to: str | Sequence[str]
    ) -> Relation:
        """A relation from this model's key to any number of rows of ``target``."""
        from .relations import Relation

        return Relation("has_many", self, target, from_, to)

    def graph(self) -> Graph:
        """A graph rooted at this model, decoding its records alone until a
        slot is joined."""
        from .graphs import Graph, State, Table

        return Graph(State((Table(self, self.qualifier, "root", False),)))

    def describe(self) -> dict[str, Any]:
        """The declaration as data: the table, its primary key, and each
        field's column with every option it was declared with."""
        return {
            "schema": self.table.schema,
            "table": self.table.name,
            "alias": self.table.alias,
            "columns": {
                name: {
                    "column": column.name,
                    "type": column._null.snapshot()["type"],
                    "nullable": column.nullable,
                    "primary_key": column.primary_key,
                    "default": column.default,
                    "generated": column.generated,
                    "values": None if column.values is None else list(column.values),
                }
                for name, column in self.columns.items()
            },
            "primary_key": list(self.primary_key),
            "path": "runtime statements",
        }
