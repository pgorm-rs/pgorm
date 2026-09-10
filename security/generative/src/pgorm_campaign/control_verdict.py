"""A control is valid only when its passing baseline and intended failure exist."""

import copy

from . import comparison, oracles
from .control_catalog import REQUIRED_FAMILIES


def baseline(spec, report):
    expected = "expected-rejection" if spec.id == "error-cause" else "pass"
    if (
        report.get("status") != expected
        or report.get("program_sha256") != spec.program.digest
    ):
        raise comparison.InvalidOracle(
            "control baseline did not pass its declared native program"
        )
    if any(
        report[side].get("program_sha256") != spec.program.digest
        for side in ("subject", "reference")
    ):
        raise comparison.InvalidOracle(
            "control baseline observations belong to a different program"
        )
    checks = oracles.compare(
        spec.program.data(),
        report["subject"],
        report["reference"],
        report["subject_state"],
        report["reference_state"],
    )
    if len(checks) != len(spec.program.data()["observations"]) or not all(
        item["equal"] for item in checks
    ):
        raise comparison.InvalidOracle(
            "control baseline has missing or unequal observations"
        )


def subject(report, identity):
    result = {
        "program_sha256": report["program_sha256"],
        "status": report["status"],
        "steps": copy.deepcopy(report["steps"]),
        "cleanup_errors": [],
        "control": {"id": identity, "dispatched": True},
    }
    for step in result["steps"]:
        step["native_paths"] = []
    return result


def assess(spec, report):
    if report.get("dispatch_count") != 1:
        return (
            "invalid-control",
            "control implementation was not dispatched exactly once",
        )
    changed = comparison.encoded(report["control"]["steps"]) != comparison.encoded(
        subject(report["baseline"]["subject"], spec.id)["steps"]
    )
    changed |= comparison.encoded(report["subject_state"]) != comparison.encoded(
        report["baseline"]["subject_state"]
    )
    if not changed:
        return "invalid-control", "control made no observable change"
    failed = [
        (item["step"], item["reason"])
        for item in report["comparisons"]
        if not item["equal"]
    ]
    if failed != [(spec.expected_step, spec.expected_reason)]:
        return (
            "invalid-control",
            "oracle did not detect exactly the intended control failure",
        )
    return "valid-control", "intended defect detected"


def verified(spec, report):
    try:
        baseline(spec, report["baseline"])
        if report["program_sha256"] != spec.program.digest or any(
            report[side]["program_sha256"] != spec.program.digest
            for side in ("control", "reference")
        ):
            return False
        checks = oracles.compare(
            spec.program.data(),
            report["control"],
            report["reference"],
            report["subject_state"],
            report["reference_state"],
            control=spec.id,
        )
        return (
            report.get("status") == "valid-control"
            and checks == report["comparisons"]
            and assess(spec, report)[0] == "valid-control"
        )
    except (KeyError, TypeError, ValueError):
        return False


def summary(specs, reports, *, required=REQUIRED_FAMILIES):
    expected = [spec.id for spec in specs]
    actual = [report.get("id") for report in reports]
    families = {spec.family for spec in specs}
    complete = (
        bool(expected) and len(set(expected)) == len(expected) and expected == actual
    )
    complete &= required <= families
    complete &= all(verified(spec, report) for spec, report in zip(specs, reports))
    return {
        "passed": complete,
        "expected": len(expected),
        "executed": len(actual),
        "families": sorted(families),
        "missing_families": sorted(required - families),
        "results": [
            {
                "id": report.get("id"),
                "status": report.get("status"),
                "reason": report.get("reason"),
            }
            for report in reports
        ],
    }
