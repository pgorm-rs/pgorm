from typing import ClassVar
from ._native import Value
from ._expressions import Expr

class Identifier:
    def __init__(self, name: str | Identifier) -> None: ...
    @property
    def name(self) -> str: ...

class Direction:
    Asc: ClassVar[Direction]
    Desc: ClassVar[Direction]

class Nulls:
    First: ClassVar[Nulls]
    Last: ClassVar[Nulls]

class Compiled:
    @property
    def sql(self) -> str: ...
    @property
    def params(self) -> list[Value]: ...

class LikePattern:
    def __init__(self, pattern: str, *, escape: str | None = ...) -> None: ...

class AliasedExpr:
    @property
    def expr(self) -> Expr: ...
    @property
    def alias(self) -> Identifier: ...

class OrderBy:
    def __init__(self, expr: Expr, direction: Direction, *, nulls: Nulls | None = ...) -> None: ...
    @property
    def expr(self) -> Expr: ...
    @property
    def direction(self) -> Direction: ...
    @property
    def nulls(self) -> Nulls | None: ...

