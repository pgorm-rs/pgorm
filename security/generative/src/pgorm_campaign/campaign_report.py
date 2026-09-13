"""The campaign report, whose field names keep the run classes apart.

There is no total number of programs in this document, and that is the point.
A constructor call that never opened a connection and a program an independent
oracle decided are counted in fields that cannot be confused for one another
(`construction_only_programs_constructed` against
`live_database_programs_checked`), and compile evidence keeps the `compile_`
prefix it already had. Nothing adds them up, so nothing can report a million
constructor calls as a million independently checked database programs.
"""

import json

VERSION = 1

# Text that must never reach a retained document. The fixture mints a password
# per run; it travels in the environment and is recorded nowhere.
SECRETS = (
    "postgresql://",
    "postgres://",
    "password=",
    "PGPASSWORD",
    "POSTGRES_PASSWORD",
)

CLASS_FIELDS = {
    "construction": "construction_only_programs_constructed",
    "runtime": "live_database_programs_checked",
    "invalid": "invalid_input_programs_checked",
    "control": "control_programs_checked",
    "compile": "compile_suite_invocations",
}


def _class_records(records, name):
    return [record for record in records if record.get("run_class") == name]


# [spec:pgorm:req:generative.profiles]
# [spec:pgorm:req:generative.verdict]
def counts(plan, records, verdicts):
    """Per-class counts under names that state which claim they support."""
    result = {"note": "run classes are counted separately and are never summed"}
    for name, field in CLASS_FIELDS.items():
        scheduled = len(plan.by_class(name))
        recorded = _class_records(records, name)
        if not scheduled and not recorded:
            continue
        result[field] = {
            "scheduled": scheduled,
            "recorded": len(recorded),
            "verdicts": verdicts.get(name, {}),
            "reached_database": name in ("runtime", "invalid", "control"),
            "oracle_decided": name in ("runtime", "invalid", "control"),
        }
    return result


def _native_evidence(records):
    """How much of the run actually dispatched through the native binding."""
    live = [
        record
        for record in records
        if record.get("run_class") in ("runtime", "invalid", "control")
    ]
    paths = set()
    for record in live:
        paths.update(record.get("native_paths") or ())
    return {
        "live_items_with_native_dispatch": sum(
            1 for record in live if record.get("native_paths")
        ),
        "live_items": len(live),
        "distinct_native_paths": len(paths),
        "native_paths": sorted(paths),
    }


def _findings(records):
    return [
        {
            "item": record["id"],
            "run_class": record["run_class"],
            "verdict": record["verdict"],
            "directory": (record.get("finding") or {})
            .get("files", {})
            .get("directory"),
            "program_sha256": record.get("program_sha256"),
            "reduced": (record.get("finding") or {}).get("reduced"),
            "commands": (record.get("finding") or {}).get("commands"),
        }
        for record in records
        if record.get("finding")
    ]


# [spec:pgorm:req:generative.artifacts]
def credentials_absent(document):
    """Scan the assembled document for anything that looks like a credential."""
    text = json.dumps(document, default=str)
    return sorted({secret for secret in SECRETS if secret in text})


# [spec:pgorm:req:generative.artifacts]
# [spec:pgorm:req:generative.verdict]
def assemble(
    profile,
    plan,
    records,
    *,
    identity,
    postgres,
    coverage,
    controls,
    compile_document,
    verdicts,
    timing,
    aggregate,
    artifacts,
):
    """Build the retained run document from evidence the run actually produced."""
    declared = identity.get("profile") or profile.identity()
    document = {
        "version": VERSION,
        "kind": "campaign-run",
        "passed": bool(aggregate["passed"]),
        "profile": declared,
        "class_claims": identity.get("class_claims", {}),
        "source": identity.get("source", {"revision": None, "dirty": None}),
        "versions": identity.get("versions", {}),
        "pins": identity.get("pins", {}),
        "builds": {
            **identity.get("builds", {}),
            # The counts above belong to the separate build command. This one
            # is the campaign's own: a run that compiled the extension per
            # program would not be amortized, and the number says so directly.
            "extension_builds_during_campaign": sum(
                int(record.get("builds") or 0) for record in records
            ),
        },
        "postgres": postgres,
        "seeds": {
            "policy": declared["seed_policy"],
            "items": sorted(
                {record["seed"] for record in records if record.get("seed") is not None}
            ),
        },
        "work": {
            "scheduled_total": len(plan.items),
            "recorded_total": len(records),
            "scheduled_by_class": plan.counts(),
            "verdicts_by_class": verdicts,
        },
        "counts": counts(plan, records, verdicts),
        "compile": _compile_section(profile, compile_document),
        "controls": controls,
        "coverage": coverage,
        "native_evidence": _native_evidence(records),
        "timing": timing,
        "findings": _findings(records),
        "aggregate": aggregate,
        "artifacts": artifacts,
        "records": records,
    }
    leaked = credentials_absent(document)
    document["credentials_omitted"] = not leaked
    if leaked:
        document["passed"] = False
        document["aggregate"]["faults"].append(
            {
                "kind": "artifact-malformed",
                "detail": "report contains credential text",
                "item": None,
            }
        )
    return document


def _compile_section(profile, document):
    """Compile counts stay in their own section, with their own vocabulary."""
    if not profile.included("compile"):
        return {
            "included": False,
            "reason": profile.klass("compile").get("reason", ""),
            "compile_cases": 0,
        }
    if not isinstance(document, dict):
        return {"included": True, "available": False, "compile_cases": 0}
    return {
        "included": True,
        "available": True,
        "passed": document.get("passed"),
        **{key: value for key, value in document.items() if key.startswith("compile_")},
        "unattributed": document.get("unattributed"),
        "defects": len(document.get("defects", ())),
        "faults": len(document.get("faults", ())),
    }


# [spec:pgorm:req:generative.verdict]
def render(document):
    """A short human summary; the JSON document remains the evidence."""
    lines = [
        "campaign {profile[name]} v{profile[profile_version]} "
        "({source[revision]}{dirty})".format(
            **document, dirty=", dirty" if document["source"]["dirty"] else ""
        ),
        "  scheduled {work[scheduled_total]}, recorded {work[recorded_total]}".format(
            **document
        ),
    ]
    for field, entry in document["counts"].items():
        if field == "note":
            continue
        verdicts = ", ".join(
            f"{name} {count}"
            for name, count in sorted(entry["verdicts"].items())
            if count
        )
        lines.append(
            f"  {field}: {entry['recorded']}/{entry['scheduled']}"
            + (f" [{verdicts}]" if verdicts else "")
        )
    coverage = document["coverage"]
    lines.append(
        "  coverage {}: {}/{} declared, {} full-matrix obligations outstanding".format(
            coverage["obligations"],
            coverage["declared_satisfied"],
            coverage["declared_total"],
            coverage["full_matrix"]["outstanding"],
        )
    )
    compile_section = document["compile"]
    lines.append(
        "  compile: "
        + (
            "excluded (" + compile_section.get("reason", "") + ")"
            if not compile_section["included"]
            else str(compile_section.get("compile_cases", 0)) + " cases"
        )
    )
    for entry in document["aggregate"]["faults"]:
        lines.append(f"  FAULT {entry['kind']}: {entry['detail']}")
    for finding in document["findings"]:
        lines.append(f"  FINDING {finding['item']} -> {finding['directory']}")
    lines.append("  passed" if document["passed"] else "  FAILED")
    return "\n".join(lines)


__all__ = [
    "CLASS_FIELDS",
    "SECRETS",
    "VERSION",
    "assemble",
    "counts",
    "credentials_absent",
    "render",
]
