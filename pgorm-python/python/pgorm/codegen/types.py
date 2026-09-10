"""Map reflected standard Rust field spellings to their native Python values."""

from dataclasses import dataclass
from collections.abc import Iterable
from typing import Any


@dataclass(frozen=True)
class FieldType:
    annotation: str
    kind: str | None = None
    array: bool = False
    nullable: bool = False
    json: bool = False

    def binding(self, variable: str = "value") -> str:
        if self.kind is None:
            return variable
        if self.array:
            expression = f"p.Value.array({self.kind}, {variable})"
        elif self.json:
            expression = f"p.Value.json({variable})"
            if self.nullable:
                expression = (
                    f"p.Value.null('json') if {variable} is None else {expression}"
                )
        else:
            expression = f"p.Value({variable}, {self.kind})"
        return f"{variable} if isinstance({variable}, p.Value) else ({expression})"


SCALARS = {
    "bool": ("bool", "bool"),
    "i8": ("int", "i8"),
    "i16": ("int", "i16"),
    "i32": ("int", "i32"),
    "i64": ("int", "i64"),
    "u32": ("int", "u32"),
    "u64": ("int", "u64"),
    "f32": ("float", "f32"),
    "f64": ("float", "f64"),
    "String": ("str", "text"),
    "std::string::String": ("str", "text"),
    "alloc::string::String": ("str", "text"),
    "char": ("str", "char"),
    "Decimal": ("Decimal", "decimal"),
    "rust_decimal::Decimal": ("Decimal", "decimal"),
    "Uuid": ("UUID", "uuid"),
    "uuid::Uuid": ("UUID", "uuid"),
    "NaiveDate": ("date", "date"),
    "chrono::NaiveDate": ("date", "date"),
    "NaiveTime": ("time", "time"),
    "chrono::NaiveTime": ("time", "time"),
    "NaiveDateTime": ("datetime", "datetime"),
    "chrono::NaiveDateTime": ("datetime", "datetime"),
    "IpNetwork": ("str", "ipnetwork"),
    "ipnetwork::IpNetwork": ("str", "ipnetwork"),
    "MacAddress": ("bytes", "mac_address"),
    "mac_address::MacAddress": ("bytes", "mac_address"),
    "Vector": ("list[float]", "vector"),
    "pgvector::Vector": ("list[float]", "vector"),
}


def wrapped(spelling: str, names: Iterable[str]) -> str | None:
    for name in names:
        prefix = name + "<"
        if spelling.startswith(prefix) and spelling.endswith(">"):
            return spelling[len(prefix) : -1]
    return None


def field_type(column: dict[str, Any]) -> FieldType:
    spelling = column.get("rust_decode_type")
    if not isinstance(spelling, str):
        return FieldType("Any")
    return resolve(spelling.replace(" ", ""), column["input_hint"])


def resolve(spelling: str, hint: dict[str, Any]) -> FieldType:
    inner = wrapped(spelling, ("Option", "std::option::Option", "core::option::Option"))
    if inner is not None:
        resolved = resolve(inner, hint)
        return FieldType(
            resolved.annotation + " | None",
            resolved.kind,
            resolved.array,
            True,
            resolved.json,
        )
    if spelling in ("Vec<u8>", "std::vec::Vec<u8>", "alloc::vec::Vec<u8>"):
        return FieldType("bytes", repr("bytes"))
    inner = wrapped(spelling, ("Vec", "std::vec::Vec", "alloc::vec::Vec"))
    if inner is not None:
        if hint.get("kind") != "array":
            return FieldType("Any")
        element = resolve(inner, hint["element"])
        if element.kind is None or element.array:
            return FieldType("Any")
        return FieldType(f"list[{element.annotation}]", element.kind, array=True)
    if hint.get("kind") == "enum":
        return FieldType(
            "str", f"p.TypeName({hint['name']!r}, schema={hint.get('schema')!r})"
        )
    if spelling in ("Json", "JsonValue", "serde_json::Value"):
        return FieldType("Any", repr("json"), json=True)
    timezone = wrapped(spelling, ("DateTime", "chrono::DateTime"))
    if timezone is not None and timezone in (
        "Utc",
        "chrono::Utc",
        "Local",
        "chrono::Local",
        "FixedOffset",
        "chrono::FixedOffset",
    ):
        kind = {
            "Utc": "datetime_utc",
            "Local": "datetime_local",
            "FixedOffset": "datetime_fixed",
        }[timezone.rsplit("::", 1)[-1]]
        return FieldType("datetime", repr(kind))
    if spelling in SCALARS:
        annotation, kind = SCALARS[spelling]
        return FieldType(annotation, repr(kind))
    return FieldType("Any")
