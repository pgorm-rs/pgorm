"""The whole compile case list, tied to the obligations it exists to discharge.

`matrix.obligations()` deliberately omits the `outside_runtime` block: those
entries name surfaces no runtime program can reach, and folding them into the
runtime requirement set would let a compile result inflate a runtime count.
This module reads the same block from the other side. Every `compile-only`
entry must be claimed by at least one case here, and no case may claim an entry
that is not `compile-only` — the separation only holds if it is checked in both
directions.
"""

from . import compile_codegen, compile_entities, compile_generics, compile_ownership
from .compile_case import CompileCaseError, validated
from .matrix import load

SOURCES = (compile_entities, compile_generics, compile_ownership, compile_codegen)


def compile_only():
    """The `outside_runtime` entries a compile suite is expected to discharge."""
    return {
        entry["id"]: entry
        for entry in load()["outside_runtime"]
        if entry["status"] == "compile-only"
    }


# [spec:pgorm:req:generative.compile-suite]
def cases():
    """Every generated case, validated for unique identity and live obligation."""
    collected = []
    for module in SOURCES:
        collected.extend(module.cases())
    suite = validated(collected)
    declared = compile_only()
    for case in suite:
        if case.obligation not in declared:
            raise CompileCaseError(
                "case claims an obligation that is not compile-only: " + case.id
            )
    claimed = {case.obligation for case in suite}
    unclaimed = sorted(set(declared) - claimed)
    if unclaimed:
        raise CompileCaseError("no case discharges " + unclaimed[0])
    return suite


# [spec:pgorm:req:generative.compile-suite]
def coverage(results):
    """Per-obligation compile coverage, computed from observed verdicts only.

    An obligation is discharged when every case claiming it landed on the
    verdict it predicted. A scheduled case proves nothing; only a build does.
    """
    declared = compile_only()
    summary = {}
    for identity, entry in declared.items():
        summary[identity] = {
            "rust": list(entry["rust"]),
            "reason": entry["reason"],
            "cases": 0,
            "satisfied": 0,
            "discharged": False,
        }
    for result in results:
        bucket = summary.get(result["obligation"])
        if bucket is None:
            continue
        bucket["cases"] += 1
        if result["status"] in SATISFIED:
            bucket["satisfied"] += 1
    for bucket in summary.values():
        bucket["discharged"] = (
            bucket["cases"] > 0 and bucket["cases"] == bucket["satisfied"]
        )
    return summary


# The verdicts that mean the case did what it claimed it would do. Everything
# else — a toolchain failure, a generator that failed where source was wanted,
# a rejection that named something other than what was predicted — leaves the
# obligation undischarged.
SATISFIED = frozenset(
    {"compiled", "expected-rejection", "expected-refusal", "generated"}
)


def split(cases_):
    """Separate codegen cases, which must visit the generator first."""
    source = [case for case in cases_ if case.kind == "source"]
    codegen = [case for case in cases_ if case.kind == "codegen"]
    return source, codegen


__all__ = ["SATISFIED", "SOURCES", "cases", "compile_only", "coverage", "split"]
