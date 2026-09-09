"""Detached declared-field views over native PostgreSQL records."""

from __future__ import annotations

from collections.abc import Iterator, Mapping
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from .. import _native as p

if TYPE_CHECKING:
    from .model import Model


# [spec:pgorm:req:python.models]
@dataclass(frozen=True, eq=False, slots=True)
class ModelRecord(Mapping[str, Any]):
    model: Model
    native: p.Record
    selected: tuple[str, ...]

    def __post_init__(self) -> None:
        from .model import Model

        if not isinstance(self.model, Model) or not isinstance(self.native, p.Record):
            raise p.ConstructionError(
                "model records require a Model and native PostgreSQL Record"
            )
        selected = self.model._selection(self.selected)
        object.__setattr__(self, "selected", selected)
        if self.native.keys() != selected:
            raise p.DecodeError(
                "record field identities differ from the declared model selection"
            )
        for name in selected:
            self.model.columns[name].validate(self.native.tagged(name), decoding=True)

    def __getitem__(self, name: str) -> Any:
        return self.native[name]

    def __iter__(self) -> Iterator[str]:
        return iter(self.native)

    def __len__(self) -> int:
        return len(self.native)

    def tagged(self, name: str) -> p.Value:
        return self.native.tagged(name)
