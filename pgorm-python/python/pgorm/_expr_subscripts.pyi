# Stub-only: one family of `Expr`'s methods, mixed into `Expr` for the type
# checker. The runtime class defines every method itself.
from typing import Any
from ._expressions import Expr

class _ExprSubscripts:
    def at(self, /, index: Any) -> Expr: ...
    def slice(self, /, lower: Any = ..., upper: Any = ...) -> Expr: ...
