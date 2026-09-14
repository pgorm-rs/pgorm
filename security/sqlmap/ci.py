#!/usr/bin/env python3
"""Publish a profile's runner verdict, including an explicit not-run status."""
import json
import os
from pathlib import Path
import sys


# [spec:pgorm:req:security.sqlmap.ci]
# [spec:pgorm:req:security.sqlmap.profiles]
def profile_status(profile, artifacts):
    report_path = artifacts / "run/report.json"
    try:
        report = json.loads(report_path.read_text())
    except FileNotFoundError:
        return "not-run", "setup failed before the runner wrote its report", {}
    except (OSError, ValueError):
        return "incomplete", "runner report is unreadable", {}
    if not isinstance(report, dict):
        return "incomplete", "runner report is malformed", {}
    if report.get("profile") != profile or report.get("subset") != []:
        return "incomplete", "report does not cover the requested complete profile", {}
    # A report that does not account for its own exemptions cannot be audited against
    # the work it claims to have scheduled, so it is evidence of nothing.
    inapplicable, falsified = report.get("inapplicable"), report.get("falsified_exemptions")
    if not isinstance(inapplicable, dict) or not isinstance(falsified, list):
        return "incomplete", "report does not account for declared inapplicability", {}
    if falsified:
        return "fail", "the scanner detected declared-inapplicable pairs: " + ", ".join(sorted(map(str, falsified))), inapplicable
    if report.get("pass") is not True:
        return "fail", "runner did not report a passing profile; inspect run/report.json", inapplicable
    if profile == "full":
        direct = report.get("direct_regressions")
        if not isinstance(direct, dict) or direct.get("pass") is not True:
            return "incomplete", "full profile lacks passing direct security regressions", inapplicable
    return "pass", f"the pinned profile passed every scheduled pair; {len(inapplicable)} pairs are declared inapplicable", inapplicable


def main():
    profile, directory = sys.argv[1:]
    if profile not in ("smoke", "full"):
        raise ValueError("profile must be smoke or full")
    artifacts = Path(directory)
    artifacts.mkdir(parents=True, exist_ok=True)
    status, reason, inapplicable = profile_status(profile, artifacts)
    declared = {key: entry.get("reason") for key, entry in sorted(inapplicable.items()) if isinstance(entry, dict)}
    (artifacts / "ci-status.json").write_text(json.dumps({"profile": profile, "status": status, "reason": reason, "inapplicable": declared}, indent=2) + "\n")
    summary = f"sqlmap / {profile}: {status.upper()}\n\n{reason}\n"
    if declared:
        summary += f"\nDeclared inapplicable ({len(declared)} pairs, excluded from scheduled work and never counted as passes):\n"
        summary += "".join(f"- {key}: {why}\n" for key, why in declared.items())
    (artifacts / "ci-summary.txt").write_text(summary)
    if destination := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(destination, "a") as output:
            output.write(summary)
    print(summary)
    return 0 if status == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
