"""Runtime column declarations and comparisons over native expressions."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from .. import _native as p

# These tags are produced by the native Record decoder without reinterpretation.
MODEL_KINDS = frozenset(
    {
        "bool",
        "i8",
        "i16",
        "i32",
        "i64",
        "u32",
        "f32",
        "f64",
        "text",
        "bytes",
        "decimal",
        "uuid",
        "json",
        "date",
        "time",
        "datetime",
        "datetime_utc",
        "ipnetwork",
        "mac_address",
        "vector",
        "int4range",
        "int8range",
        "numrange",
        "daterange",
        "tsrange",
        "tstzrange",
        "int4multirange",
        "int8multirange",
        "nummultirange",
        "datemultirange",
        "tsmultirange",
        "tstzmultirange",
    }
)


# [spec:pgorm:req:python.models]
@dataclass(frozen=True, slots=True)
class Column:
    kind: str | p.TypeName | p.CreatedRange
    name: str | None = None
    nullable: bool = False
    primary_key: bool = False
    array: bool = False
    _null: p.Value = field(init=False, repr=False, compare=False)

    def __post_init__(self) -> None:
        for flag in (self.nullable, self.primary_key, self.array):
            if type(flag) is not bool:
                raise p.ConstructionError("column flags require exact booleans")
        if self.primary_key and self.nullable:
            raise p.ConstructionError("primary-key columns cannot be nullable")
        if self.name is not None:
            object.__setattr__(self, "name", p.Identifier(self.name).name)
        if isinstance(self.kind, p.TypeName):
            if self.kind.schema is None:
                raise p.ConstructionError(
                    "enum columns require a schema-qualified TypeName"
                )
        elif isinstance(self.kind, p.CreatedRange):
            # A decoded row names its range type with the schema it lives in.
            if self.kind.schema is None:
                raise p.ConstructionError(
                    "created range columns require a schema-qualified CreatedRange"
                )
        elif type(self.kind) is not str or self.kind not in MODEL_KINDS:
            raise p.UnsupportedCapabilityError(
                "column kind is not supported by the native Record decoder"
            )
        kind = self.kind
        if not self.array:
            null = p.Value.null(kind)
        elif isinstance(kind, p.CreatedRange):
            raise p.ConstructionError("arrays of a created range type are not supported")
        else:
            null = p.Value.array(kind, None)
        object.__setattr__(self, "_null", null)

    def validate(self, value: p.Value, *, decoding: bool = False) -> p.Value:
        error = p.DecodeError if decoding else p.ConstructionError
        if (value.kind, value.type_name, value.created_type, value.element_type) != (
            self._null.kind,
            self._null.type_name,
            self._null.created_type,
            self._null.element_type,
        ):
            raise error("value type differs from the declared model column")
        if value.is_null and not self.nullable:
            raise error("non-nullable model column received SQL NULL")
        return value

    def value(self, data: Any) -> p.Value:
        kind = self.kind
        if isinstance(data, p.Value):
            result = data
        elif self.array and not isinstance(kind, p.CreatedRange):
            result = p.Value.array(kind, data)
        else:
            result = p.Value(data, kind)
        return self.validate(result)

    def describe(self) -> dict[str, Any]:
        return {
            "column": self.name,
            "type": self._null.snapshot()["type"],
            "nullable": self.nullable,
            "primary_key": self.primary_key,
        }


@dataclass(frozen=True, eq=False, slots=True)
class ModelColumn:
    field_name: str
    declaration: Column
    expression: p.Expr

    def __post_init__(self) -> None:
        if not isinstance(self.declaration, Column) or not isinstance(
            self.expression, p.Expr
        ):
            raise p.ConstructionError("model columns require a Column and native Expr")
        object.__setattr__(self, "field_name", p.Identifier(self.field_name).name)

    def expr(self) -> p.Expr:
        return self.expression

    def _operand(self, value: Any) -> p.Expr | p.Value:
        if isinstance(value, ModelColumn):
            if (
                self.declaration._null.snapshot()["type"]
                != value.declaration._null.snapshot()["type"]
            ):
                raise p.ConstructionError(
                    "compared model columns have different declared types"
                )
            return value.expression
        if isinstance(value, p.Expr):
            return value
        return self.declaration.value(value)

    def eq(self, value: Any) -> p.Expr:
        return self.expression.eq(self._operand(value))

    def ne(self, value: Any) -> p.Expr:
        return self.expression.ne(self._operand(value))

    def gt(self, value: Any) -> p.Expr:
        return self.expression.gt(self._operand(value))

    def gte(self, value: Any) -> p.Expr:
        return self.expression.gte(self._operand(value))

    def lt(self, value: Any) -> p.Expr:
        return self.expression.lt(self._operand(value))

    def lte(self, value: Any) -> p.Expr:
        return self.expression.lte(self._operand(value))

    def __eq__(self, value: object) -> p.Expr:  # type: ignore[override]
        return self.eq(value)

    def __ne__(self, value: object) -> p.Expr:  # type: ignore[override]
        return self.ne(value)

    def __gt__(self, value: Any) -> p.Expr:
        return self.gt(value)

    def __ge__(self, value: Any) -> p.Expr:
        return self.gte(value)

    def __lt__(self, value: Any) -> p.Expr:
        return self.lt(value)

    def __le__(self, value: Any) -> p.Expr:
        return self.lte(value)

    def is_null(self) -> p.Expr:
        return self.expression.is_null()

    def is_not_null(self) -> p.Expr:
        return self.expression.is_not_null()

    def is_in(self, values: list[Any] | tuple[Any, ...]) -> p.Expr:
        if type(values) not in (list, tuple):
            raise p.ConstructionError("model IN requires a list or tuple")
        return self.expression.is_in([self._operand(value) for value in values])

    def asc(self, *, nulls: p.Nulls | None = None) -> p.OrderBy:
        return self.expression.asc(nulls=nulls)

    def desc(self, *, nulls: p.Nulls | None = None) -> p.OrderBy:
        return self.expression.desc(nulls=nulls)

    def __bool__(self) -> bool:
        raise p.ConstructionError("model columns cannot be tested as Python booleans")
