"""Application-defined model metadata, lowered through native statement builders."""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass, replace
from types import MappingProxyType
from typing import TYPE_CHECKING, Any, cast

from .. import _native as p
from .columns import Column, ModelColumn

if TYPE_CHECKING:
    from .queries import ModelQuery, ModelWrite


# [spec:pgorm:req:python.models]
@dataclass(frozen=True, eq=False, slots=True, init=False)
class Model:
    table: p.Table
    columns: Mapping[str, Column]
    primary_keys: tuple[str, ...]

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
        if not isinstance(columns, Mapping) or not columns:
            raise p.ConstructionError("model columns require a non-empty mapping")
        declarations = {}
        physical = set()
        for identity, column in columns.items():
            identity = p.Identifier(identity).name
            if identity in declarations:
                raise p.ConstructionError("duplicate model field identity")
            if not isinstance(column, Column):
                raise p.ConstructionError(
                    "each model field requires a Column declaration"
                )
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
            "primary_keys",
            tuple(name for name, column in declarations.items() if column.primary_key),
        )

    def as_(self, alias: str) -> Model:
        return Model(self.table.as_(alias), self.columns)

    def col(self, field: str) -> ModelColumn:
        name = p.Identifier(field).name
        if name not in self.columns:
            raise p.ConstructionError("unknown model field")
        declaration = self.columns[name]
        assert declaration.name is not None
        return ModelColumn(name, declaration, self.table.col(declaration.name))

    def _selection(self, fields: tuple[str, ...]) -> tuple[str, ...]:
        selected = tuple(self.columns) if not fields else fields
        names = tuple(self.col(name).field_name for name in selected)
        if len(set(names)) != len(names):
            raise p.ConstructionError("duplicate model output identity")
        return names

    def _projection(self, fields: tuple[str, ...]) -> tuple[p.AliasedExpr, ...]:
        return tuple(self.col(name).expression.as_(name) for name in fields)

    def select(self, *fields: str) -> ModelQuery:
        from .queries import ModelQuery

        selected = self._selection(fields)
        statement = p.Select(*self._projection(selected)).from_(self.table)
        return ModelQuery(self, statement, selected)

    def find(self) -> ModelQuery:
        return self.select()

    def key(self, values: Mapping[str, Any]) -> p.Condition:
        if not self.primary_keys:
            raise p.ConstructionError("model has no declared primary key")
        if not isinstance(values, Mapping) or set(values) != set(self.primary_keys):
            raise p.ConstructionError(
                "key lookup requires every declared primary-key field and no others"
            )
        return p.Condition.all(
            *(self.col(name).eq(values[name]) for name in self.primary_keys)
        )

    def _assignments(
        self, values: Mapping[str, Any]
    ) -> tuple[tuple[str, p.Value], ...]:
        if not isinstance(values, Mapping):
            raise p.ConstructionError("model writes require a field mapping")
        for name in values:
            self.col(name)
        # Use declaration order so mapping order cannot change SQL/parameter layout.
        return tuple(
            (cast(str, column.name), column.value(values[name]))
            for name, column in self.columns.items()
            if name in values
        )

    def insert(self, values: Mapping[str, Any]) -> ModelWrite:
        from .queries import ModelWrite

        if self.table.alias is not None:
            raise p.ConstructionError("model inserts require an unaliased table")
        assignments = self._assignments(values)
        statement = p.Insert(self.table)
        if assignments:
            statement = statement.columns(*(name for name, _ in assignments)).values(
                *(value for _, value in assignments)
            )
        else:
            statement = statement.default_values()
        return ModelWrite(self, statement)

    def update(self, values: Mapping[str, Any]) -> ModelWrite:
        from .queries import ModelWrite

        assignments = self._assignments(values)
        if not assignments:
            raise p.ConstructionError(
                "model update requires at least one assigned field"
            )
        statement = p.Update(self.table)
        for column, value in assignments:
            statement = statement.set(column, value)
        return ModelWrite(self, statement)

    def delete(self) -> ModelWrite:
        from .queries import ModelWrite

        return ModelWrite(self, p.Delete(self.table))

    def describe(self) -> dict[str, Any]:
        return {
            "schema": self.table.schema,
            "table": self.table.name,
            "alias": self.table.alias,
            "columns": {
                name: column.describe() for name, column in self.columns.items()
            },
            "primary_keys": list(self.primary_keys),
            "path": "runtime statements",
        }
