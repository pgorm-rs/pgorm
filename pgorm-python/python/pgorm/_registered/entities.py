"""Owned Python views used by generated concrete registration modules."""

from collections.abc import Mapping
from typing import Any, Generic, TypeVar

from .. import _native as native_types

M = TypeVar("M", bound="ModelView")
A = TypeVar("A", bound="ActiveView")


class ModelView(Mapping[str, Any], Generic[A]):
    __slots__ = ("_model",)
    _entity_name: str
    _active_class: type[A]

    def __init__(self, model: native_types.EntityModel):
        if not isinstance(model, native_types.EntityModel):
            raise native_types.ConstructionError("model view requires a native EntityModel")
        if model.entity_name != self._entity_name:
            raise native_types.LifecycleError("model belongs to another entity registration")
        self._model = model

    @property
    def native(self):
        return self._model

    @property
    def entity_name(self) -> str:
        return self._model.entity_name

    def __getitem__(self, key: str) -> Any:
        return self._model[key]

    def __iter__(self):
        return iter(self._model)

    def __len__(self) -> int:
        return len(self._model)

    def tagged(self, key: str) -> native_types.Value:
        return self._model.tagged(key)

    def with_value(self: M, column, value) -> M:
        return type(self)(self._model.with_value(column, value))

    def into_active(self) -> A:
        return self._active_class(self._model.into_active())


class ActiveView(Generic[M]):
    __slots__ = ("_active",)
    _entity_name: str
    _model_class: type[M]

    def __init__(self, active: native_types.ActiveModel):
        if not isinstance(active, native_types.ActiveModel):
            raise native_types.ConstructionError("active view requires a native ActiveModel")
        if active.entity_name != self._entity_name:
            raise native_types.LifecycleError(
                "ActiveModel belongs to another entity registration"
            )
        self._active = active

    @property
    def native(self):
        return self._active

    @property
    def entity_name(self) -> str:
        return self._active.entity_name

    def get(self, column):
        return self._active.get(column)

    def set(self: A, column, value) -> A:
        return type(self)(self._active.set(column, value))

    def not_set(self: A, column) -> A:
        return type(self)(self._active.not_set(column))

    def reset(self: A, column) -> A:
        return type(self)(self._active.reset(column))

    async def insert(self, connection) -> M:
        return self._model_class(await self._active.insert(connection))

    async def update(self, connection) -> M:
        return self._model_class(await self._active.update(connection))

    async def delete(self, connection) -> int:
        return await self._active.delete(connection)


class EntityView(Generic[M, A]):
    __slots__ = ("_entity", "_model_class", "_active_class")

    def __init__(self, entity: native_types.Entity, model: type[M], active: type[A]):
        if entity.name != model._entity_name or entity.name != active._entity_name:
            raise native_types.LifecycleError(
                "generated wrapper types belong to another registration"
            )
        self._entity, self._model_class, self._active_class = entity, model, active

    @property
    def name(self) -> str:
        return self._entity.name

    def describe(self):
        return self._entity.describe()

    def col(self, name):
        return self._entity.col(name)

    def find(self) -> "QueryView[M]":
        return QueryView(self._entity.find(), self._model_class)

    def active(self) -> A:
        return self._active_class(self._entity.active())


class QueryView(Generic[M]):
    __slots__ = ("_query", "_model_class")

    def __init__(self, query: native_types.EntityQuery, model: type[M]):
        if query.entity_name != model._entity_name:
            raise native_types.LifecycleError("query belongs to another entity registration")
        self._query, self._model_class = query, model

    def filter(self, predicate) -> "QueryView[M]":
        return QueryView(self._query.filter(predicate), self._model_class)

    def order_by(self, *ordering) -> "QueryView[M]":
        return QueryView(self._query.order_by(*ordering), self._model_class)

    def limit(self, value=None) -> "QueryView[M]":
        return QueryView(self._query.limit(value), self._model_class)

    def offset(self, value=None) -> "QueryView[M]":
        return QueryView(self._query.offset(value), self._model_class)

    def inspect(self, *, terminal="all"):
        return self._query.inspect(terminal=terminal)

    async def all(self, connection) -> list[M]:
        return [self._model_class(row) for row in await self._query.all(connection)]

    async def one(self, connection) -> M:
        return self._model_class(await self._query.one(connection))

    async def one_opt(self, connection) -> M | None:
        row = await self._query.one_opt(connection)
        return None if row is None else self._model_class(row)

    def __bool__(self) -> bool:
        return bool(self._query)
