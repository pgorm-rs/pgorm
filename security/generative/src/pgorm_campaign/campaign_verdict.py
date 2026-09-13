"""One recorded verdict per scheduled item, and the faults that fail a run.

The five verdicts are closed. An item that produced something else — a crash,
a panic that escaped the binding, a report nobody could parse — is recorded as
`incomplete` and additionally raises a fault, because "we could not tell" is
not a result and must never be silently absorbed into a pass rate.

Faults are kept apart from verdicts deliberately. A verdict says what happened
to one program; a fault says the run itself is not trustworthy. Aggregate
success requires both: every item accounted for, and no fault at all.
"""

from . import profiles

VERDICTS = ("pass", "defect", "expected-rejection", "invalid-control", "incomplete")

# Verdicts that are an acceptable outcome for an item that declared them.
ACCEPTED = {"pass", "expected-rejection"}

FAULTS = (
    "empty-discovery",
    "missing-worker",
    "skipped-work",
    "duplicate-record",
    "unrecorded-verdict",
    "unexpected-verdict",
    "unexpected-error",
    "native-panic",
    "deadline",
    "artifact-missing",
    "artifact-malformed",
    "cleanup-failure",
    "coverage-unsatisfied",
    "controls-unsuccessful",
    "unamortized-build",
)


def fault(kind, detail, *, item=None):
    if kind not in FAULTS:
        raise ValueError("unknown campaign fault kind: " + kind)
    return {"kind": kind, "detail": detail, "item": item}


# [spec:pgorm:req:generative.verdict]
def from_checker(report):
    """Map an independent oracle report onto exactly one recorded verdict."""
    status = (report or {}).get("status")
    return (
        status if status in VERDICTS and status != "invalid-control" else "incomplete"
    )


# [spec:pgorm:req:generative.verdict]
def from_control(report):
    """A control that detected its declared defect passes; a broken one does not."""
    status = (report or {}).get("status")
    if status == "valid-control":
        return "pass"
    if status == "invalid-control":
        return "invalid-control"
    return "incomplete"


# [spec:pgorm:req:generative.verdict]
def from_construction(outcome):
    """Construction is a constructor call, and its verdict says only that."""
    return "pass" if (outcome or {}).get("constructed") else "incomplete"


# [spec:pgorm:req:generative.verdict]
def from_compile(document):
    """Compile evidence keeps its own vocabulary and never joins runtime counts."""
    if not isinstance(document, dict):
        return "incomplete"
    if document.get("faults"):
        return "incomplete"
    if document.get("defects"):
        return "defect"
    return "pass" if document.get("passed") else "incomplete"


def accepted(record):
    """Whether the recorded verdict is the outcome the item declared."""
    return record.get("verdict") == record.get("expected")


def _panicked(record):
    error = record.get("error") or {}
    return error.get("class") == "UnexpectedNativePanic"


def _item_faults(record):
    found = []
    verdict = record.get("verdict")
    identity = record.get("id")
    if verdict not in VERDICTS:
        found.append(fault("unrecorded-verdict", "no recorded verdict", item=identity))
        return found
    if not accepted(record):
        found.append(
            fault(
                "unexpected-verdict",
                "recorded " + verdict + ", declared " + str(record.get("expected")),
                item=identity,
            )
        )
    if _panicked(record):
        found.append(
            fault(
                "native-panic",
                str((record.get("error") or {}).get("cause")),
                item=identity,
            )
        )
    elif record.get("deadline_exceeded"):
        found.append(
            fault("deadline", "item exceeded its declared budget", item=identity)
        )
    elif verdict == "incomplete" and record.get("error"):
        found.append(fault("unexpected-error", str(record["error"]), item=identity))
    for message in record.get("cleanup_errors") or ():
        found.append(fault("cleanup-failure", str(message), item=identity))
    if record.get("builds"):
        found.append(
            fault(
                "unamortized-build",
                "an executed program compiled the extension",
                item=identity,
            )
        )
    found.extend(_artifact_faults(record))
    return found


def _artifact_faults(record):
    found = []
    for entry in record.get("artifact_faults") or ():
        kind = entry.get("kind", "artifact-malformed")
        found.append(fault(kind, entry.get("detail", ""), item=record.get("id")))
    return found


# [spec:pgorm:req:generative.verdict]
def work_faults(plan, records):
    """Compare the schedule against the record list rather than itself."""
    found = []
    if not plan.items:
        return [fault("empty-discovery", "the profile scheduled no work")]
    if not records:
        return [fault("empty-discovery", "the run produced no records")]
    seen = {}
    for record in records:
        identity = record.get("id")
        if identity in seen:
            found.append(fault("duplicate-record", "recorded twice", item=identity))
        seen[identity] = record
    for item in plan.items:
        if item.id not in seen:
            found.append(
                fault("skipped-work", "scheduled but never recorded", item=item.id)
            )
    for name in plan.counts():
        if not any(record.get("run_class") == name for record in records):
            found.append(
                fault("empty-discovery", "run class recorded nothing: " + name)
            )
    for record in records:
        found.extend(_item_faults(record))
    return found


# [spec:pgorm:req:generative.verdict]
def worker_faults(profile, records):
    """A declared worker that never recorded work is a worker that went missing."""
    live = [
        record
        for record in records
        if record.get("run_class") in profiles.LIVE_CLASSES
        and record.get("worker") is not None
    ]
    if not live:
        return []
    used = {record["worker"] for record in live}
    return [
        fault(
            "missing-worker",
            "worker recorded no live work",
            item="worker-" + str(worker),
        )
        for worker in sorted(set(range(profile.workers)) - used)
    ]


# [spec:pgorm:req:generative.verdict]
def budget_faults(profile, timings):
    """Wall-clock budgets are declared per class and for the run as a whole."""
    found = []
    for name, seconds in (timings.get("class_seconds") or {}).items():
        allowed = profile.class_seconds(name)
        if allowed and seconds > allowed:
            found.append(
                fault("deadline", name + " exceeded its declared wall-clock budget")
            )
    total = timings.get("total_seconds", 0)
    if total > profile.budgets["total_seconds"]:
        found.append(fault("deadline", "the run exceeded its total wall-clock budget"))
    return found


def counts(records):
    """Verdict tallies per run class; there is deliberately no merged total."""
    tally = {}
    for record in records:
        name = record.get("run_class", "unknown")
        bucket = tally.setdefault(name, dict.fromkeys(VERDICTS, 0))
        verdict = record.get("verdict")
        bucket[verdict if verdict in VERDICTS else "incomplete"] += 1
    return tally


# [spec:pgorm:req:generative.verdict]
def aggregate(profile, plan, records, *, coverage, controls, faults):
    """Aggregate success is conjunctive and states each conjunct separately."""
    faults = list(faults)
    if not coverage.get("satisfied"):
        faults.append(
            fault(
                "coverage-unsatisfied",
                "declared coverage obligations outstanding: "
                + str(len(coverage.get("declared_missing", ()))),
            )
        )
    if profile.document["controls"]["required"] and not controls.get("passed"):
        faults.append(
            fault(
                "controls-unsuccessful", str(controls.get("reason", "controls failed"))
            )
        )
    accounted = len(records) == len(plan.items) and all(
        accepted(record) for record in records
    )
    return {
        "passed": not faults and accounted and bool(plan.items),
        "scheduled": len(plan.items),
        "recorded": len(records),
        "all_work_accounted": accounted,
        "coverage_satisfied": bool(coverage.get("satisfied")),
        "controls_successful": bool(controls.get("passed")),
        "faults": faults,
        "fault_kinds": sorted({entry["kind"] for entry in faults}),
    }


__all__ = [
    "ACCEPTED",
    "FAULTS",
    "VERDICTS",
    "accepted",
    "aggregate",
    "budget_faults",
    "counts",
    "fault",
    "from_checker",
    "from_compile",
    "from_construction",
    "from_control",
    "work_faults",
    "worker_faults",
]
