"""The compile report, kept structurally apart from any runtime count.

Compile evidence and runtime evidence answer different questions and are not
summable. A run of this suite produces a document whose counts are all labelled
`compile_*` and which carries no runtime totals at all, so nothing downstream
can add a rejected program to a executed one and call the sum coverage.
"""

VERSION = 1

# Every terminal verdict, grouped by what it says about the library.
EXPECTED = ("compiled", "expected-rejection", "expected-refusal")
DEFECTS = ("unexpected-rejection", "unrejected", "misrejected", "unrefused")
FAULTS = ("toolchain-failure", "generator-failed", "misrefused")


def _counts(results):
    tally = {}
    for result in results:
        tally[result["status"]] = tally.get(result["status"], 0) + 1
    return dict(sorted(tally.items()))


# [spec:pgorm:req:generative.compile-suite]
def assemble(results, coverage, batches, *, seconds, identity=None):
    """Build the retained document from observed verdicts.

    `passed` is deliberately conjunctive: every case landed where it said it
    would, every obligation is discharged, and no diagnostic went unattributed.
    A run that cannot attribute a diagnostic did not prove what it thinks it
    proved, whatever its per-case tally says.
    """
    statuses = _counts(results)
    positives = [item for item in results if item["verdict"] == "accept"]
    negatives = [item for item in results if item["verdict"] == "reject"]
    refusals = [item for item in results if item["verdict"] == "refuse"]
    orphans = sum(len(batch["unattributed"]) for batch in batches)
    coded = [
        item
        for item in negatives
        if item["status"] == "expected-rejection" and item.get("coded")
    ]
    document = {
        "version": VERSION,
        "kind": "compile-coverage",
        # Named so no reader mistakes these for the runtime campaign's totals.
        "compile_cases": len(results),
        "compile_batches": len(batches),
        "compile_positive": len(positives),
        "compile_positive_compiled": sum(
            1 for item in positives if item["status"] in EXPECTED
        ),
        "compile_negative": len(negatives),
        "compile_negative_rejected": sum(
            1 for item in negatives if item["status"] == "expected-rejection"
        ),
        "compile_negative_rejected_by_code": len(coded),
        "compile_generator_refusals": len(refusals),
        "compile_generator_refused": sum(
            1 for item in refusals if item["status"] == "expected-refusal"
        ),
        "statuses": statuses,
        "defects": [item for item in results if item["status"] in DEFECTS],
        "faults": [item for item in results if item["status"] in FAULTS],
        "unattributed": orphans,
        "obligations": coverage,
        "seconds": seconds,
        "batches": [_batch_summary(batch) for batch in batches],
        "results": results,
    }
    document["passed"] = (
        not document["defects"]
        and not document["faults"]
        and orphans == 0
        and all(entry["discharged"] for entry in coverage.values())
    )
    if identity:
        document["identity"] = identity
    return document


def _batch_summary(batch):
    return {
        "name": batch["name"],
        "crate": batch["crate"],
        "verdict": batch["verdict"],
        "phase": batch["phase"],
        "cases": len(batch["results"]),
        "exit_code": batch["exit_code"],
        "seconds": batch["seconds"],
        "toolchain_failure": batch["toolchain_failure"],
        "unattributed": len(batch["unattributed"]),
    }


def render(document):
    """A short human summary; the JSON document remains the evidence."""
    lines = [
        "compile coverage (separate from runtime counts)",
        "  cases {compile_cases} in {compile_batches} crates, {seconds:.1f}s".format(
            **document
        ),
        "  positives {compile_positive_compiled}/{compile_positive} compiled".format(
            **document
        ),
        "  negatives {compile_negative_rejected}/{compile_negative} rejected "
        "({compile_negative_rejected_by_code} by error code)".format(**document),
        "  generator refusals {compile_generator_refused}/"
        "{compile_generator_refusals}".format(**document),
    ]
    for identity, entry in sorted(document["obligations"].items()):
        mark = "discharged" if entry["discharged"] else "OPEN"
        lines.append(
            f"  {identity}: {mark} ({entry['satisfied']}/{entry['cases']} cases)"
        )
    for defect in document["defects"]:
        lines.append(f"  DEFECT {defect['id']}: {defect['status']} {defect['detail']}")
    for fault in document["faults"]:
        lines.append(f"  FAULT  {fault['id']}: {fault['status']}")
    if document["unattributed"]:
        lines.append(f"  unattributed diagnostics: {document['unattributed']}")
    lines.append("  passed" if document["passed"] else "  FAILED")
    return "\n".join(lines)


__all__ = ["DEFECTS", "EXPECTED", "FAULTS", "VERSION", "assemble", "render"]
