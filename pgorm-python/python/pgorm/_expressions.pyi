from typing import Any, Never
from ._conditions import Condition as Condition
from ._query_types import Identifier
from ._expr_comparison import _ExprComparison
from ._expr_arithmetic import _ExprArithmetic
from ._expr_logic import _ExprLogic
from ._expr_predicates import _ExprPredicates
from ._expr_patterns import _ExprPatterns
from ._expr_ranges import _ExprRanges
from ._expr_search import _ExprSearch
from ._expr_shaping import _ExprShaping

class Expr(
    _ExprComparison,
    _ExprArithmetic,
    _ExprLogic,
    _ExprPredicates,
    _ExprPatterns,
    _ExprRanges,
    _ExprSearch,
    _ExprShaping,
):
    def __init__(self, _native_only: Never, /) -> None: ...

def col(name: str | Identifier, *, table: str | Identifier | None = ...,
        schema: str | Identifier | None = ...) -> Expr: ...
def bind(value: Any) -> Expr: ...
def literal(value: Any) -> Expr: ...
def call(name: str, *arguments: Any) -> Expr: ...
def tuple_expr(*expressions: Any) -> Expr: ...
def domain_value() -> Expr: ...
