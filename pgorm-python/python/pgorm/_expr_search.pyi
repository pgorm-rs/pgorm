# Stub-only: one family of `Expr`'s methods, mixed into `Expr` for the type
# checker. The runtime class defines every method itself.
from typing import Any
from ._expressions import Expr

class _ExprSearch:
    def path_exists(self, /, path: str) -> Expr: ...
    def matches(self, /, other: Any) -> Expr: ...
