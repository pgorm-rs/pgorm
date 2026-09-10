"""Versioned mandatory sensitivity controls, separate from native API coverage."""

from dataclasses import dataclass

from . import control_programs as programs
from .program import Program

VERSION = 1


@dataclass(frozen=True)
class Control:
    id: str
    family: str
    program: Program
    backend: str = "observation"
    expected_step: str = "s0"
    expected_reason: str = "ordered rows differ"


# [spec:pgorm:req:generative.controls]
def catalog():
    controls = [
        Control(
            "escaped-value",
            "value-escaping",
            programs.read("escaped-value"),
            "postgres",
        ),
        Control(
            "identifier", "identifier-escaping", programs.read("identifier"), "postgres"
        ),
        Control("predicate", "predicate-broadening", programs.read(), "postgres"),
        Control("bind-order", "bind-order", programs.read("bind-order"), "postgres"),
        Control(
            "bind-type",
            "bind-type",
            programs.typed("text", "42", "text"),
            "postgres",
            expected_reason="row multisets differ",
        ),
        Control("missing-row", "row-multiplicity", programs.read()),
        Control("duplicate-row", "row-multiplicity", programs.read()),
        Control("row-order", "row-order", programs.read()),
        Control("decode-type", "decode-types", programs.read()),
        Control(
            "optional-slot",
            "optional-joins",
            programs.structure("optional"),
            expected_reason="row multisets differ",
        ),
        Control(
            "json-null",
            "null-semantics",
            programs.typed("json", None, "jsonb"),
            expected_reason="row multisets differ",
        ),
        Control(
            "array-element",
            "arrays",
            programs.structure("array"),
            expected_reason="row multisets differ",
        ),
        Control(
            "enum-identity",
            "enums",
            programs.structure("enum"),
            expected_reason="row multisets differ",
        ),
        Control(
            "affected-count",
            "affected-counts",
            programs.write(),
            expected_reason="effect observations differ",
        ),
        Control(
            "missing-write",
            "fixture-state",
            programs.write(),
            "postgres",
            "final",
            "final fixture state differs",
        ),
        Control(
            "rollback",
            "transactions",
            programs.write(rollback=True),
            "postgres",
            "final",
            "final fixture state differs",
        ),
        Control(
            "missing-schema",
            "schema-state",
            programs.schema(),
            "postgres",
            "final",
            "final fixture state differs",
        ),
        Control(
            "stream-close",
            "stream-lifecycle",
            programs.read("stream"),
            expected_reason="stream lifecycle violates the declared operation",
        ),
        Control(
            "error-cause",
            "exact-errors",
            programs.rejection(),
            expected_reason="error cause differs",
        ),
    ]
    controls.extend(
        Control(
            name, name, programs.numeric(name), expected_reason="row multisets differ"
        )
        for name in ("float", "decimal", "temporal")
    )
    return controls


REQUIRED_FAMILIES = frozenset(
    {
        "value-escaping",
        "identifier-escaping",
        "predicate-broadening",
        "bind-order",
        "bind-type",
        "row-multiplicity",
        "row-order",
        "decode-types",
        "optional-joins",
        "null-semantics",
        "arrays",
        "enums",
        "affected-counts",
        "fixture-state",
        "transactions",
        "schema-state",
        "stream-lifecycle",
        "exact-errors",
        "float",
        "decimal",
        "temporal",
    }
)
