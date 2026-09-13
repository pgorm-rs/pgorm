#!/usr/bin/env python3
"""Publish a campaign profile's verdict, including an explicit not-run status."""

import json
import os
from pathlib import Path
import sys

# The run classes whose separation this gate enforces. A report that folded
# compile invocations into the live count would read as a far larger body of
# independently checked database programs than the campaign actually ran.
RUNTIME_FIELD = "live_database_programs_checked"
COMPILE_FIELD = "compile_suite_invocations"

# Names a summed field would plausibly take. Their absence is what keeps the
# per-class counts from being collapsed back into one headline number.
SUMMED_FIELDS = ("total", "total_programs", "programs", "programs_total", "all")


def _run_directory(artifacts):
    """The single directory a campaign minted for itself, if there is one."""
    root = artifacts / "run"
    if not root.is_dir():
        return None, None
    directories = sorted(path for path in root.iterdir() if path.is_dir())
    if not directories:
        return None, None
    if len(directories) > 1:
        return None, "several campaign run directories; the evidence is ambiguous"
    return directories[0], None


def _load(artifacts):
    """Read the run document, distinguishing absent evidence from bad evidence."""
    directory, ambiguous = _run_directory(artifacts)
    if ambiguous:
        return None, ("incomplete", ambiguous)
    # The runner mints its own run directory as its first act, so an absent
    # directory means setup never reached it. A directory with no report means
    # the campaign started and was killed or aborted, which is a failure.
    if directory is None:
        return None, ("not-run", "setup failed before the campaign reached the runner")
    try:
        report = json.loads((directory / "campaign.json").read_text())
    except FileNotFoundError:
        return None, ("fail", "the campaign started and never wrote its report")
    except (OSError, ValueError):
        return None, ("incomplete", "campaign report is unreadable")
    if not isinstance(report, dict):
        return None, ("incomplete", "campaign report is malformed")
    return report, None


def declared_profile(report):
    """The profile name the report claims, however the document spells it."""
    declared = report.get("profile")
    if isinstance(declared, dict):
        return declared.get("name")
    return declared


def compile_separation(report):
    """Why the report's compile evidence is not separable, or None if it is."""
    counts = report.get("counts")
    if not isinstance(counts, dict):
        return "report carries no per-class counts"
    if RUNTIME_FIELD not in counts:
        return "counts do not report live database programs as their own class"
    if any(field in counts for field in SUMMED_FIELDS):
        return "counts carry a summed total across run classes"
    section = report.get("compile")
    if not isinstance(section, dict) or "included" not in section:
        return "compile evidence is not reported in its own section"
    if not section["included"]:
        return None
    if not section.get("available"):
        return "the included compile suite produced no compile evidence"
    if COMPILE_FIELD not in counts:
        return "compile invocations are not counted apart from runtime programs"
    return None


def coverage_stated(report):
    """Why the report's coverage claim is unusable, or None if it stands."""
    coverage = report.get("coverage")
    if not isinstance(coverage, dict):
        return "report carries no coverage section"
    if not isinstance(coverage.get("full_matrix"), dict):
        return "report does not record its outstanding full-matrix obligations"
    return None


# [spec:pgorm:req:generative.ci]
def profile_status(profile, artifacts):
    """Convert a campaign run directory into one of four publishable states."""
    report, early = _load(Path(artifacts))
    if early:
        return early
    if declared_profile(report) != profile:
        return "incomplete", "report does not cover the requested profile"
    for reason in (compile_separation(report), coverage_stated(report)):
        if reason:
            return "incomplete", reason
    if profile == "full" and not report["compile"]["included"]:
        return "incomplete", "the full campaign must include the compile suite"
    if report.get("passed") is not True:
        return "fail", "the campaign did not pass; inspect run/*/campaign.json"
    return "pass", "the pinned profile passed"


def _document(profile, status, reason, artifacts):
    directory, _ = _run_directory(artifacts)
    return {
        "profile": profile,
        "status": status,
        "reason": reason,
        "run": str(directory) if directory else None,
    }


def main():
    profile, directory = sys.argv[1:]
    if profile not in ("smoke", "full"):
        raise ValueError("profile must be smoke or full")
    artifacts = Path(directory)
    artifacts.mkdir(parents=True, exist_ok=True)
    status, reason = profile_status(profile, artifacts)
    (artifacts / "ci-status.json").write_text(
        json.dumps(_document(profile, status, reason, artifacts), indent=2) + "\n"
    )
    summary = f"generative / {profile}: {status.upper()}\n\n{reason}\n"
    (artifacts / "ci-summary.txt").write_text(summary)
    if destination := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(destination, "a") as output:
            output.write(summary)
    print(summary)
    return 0 if status == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
