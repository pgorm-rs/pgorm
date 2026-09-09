#!/usr/bin/env python3
"""Publish a profile's runner verdict, including an explicit not-run status."""
import json
import os
from pathlib import Path
import sys


# [spec:pgorm:req:security.sqlmap.ci]
def profile_status(profile, artifacts):
    report_path = artifacts / "run/report.json"
    try:
        report = json.loads(report_path.read_text())
    except FileNotFoundError:
        return "not-run", "setup failed before the runner wrote its report"
    except (OSError, ValueError):
        return "incomplete", "runner report is unreadable"
    if not isinstance(report, dict):
        return "incomplete", "runner report is malformed"
    if report.get("profile") != profile or report.get("subset") != []:
        return "incomplete", "report does not cover the requested complete profile"
    if report.get("pass") is not True:
        return "fail", "runner did not report a passing profile; inspect run/report.json"
    if profile == "full":
        direct = report.get("direct_regressions")
        if not isinstance(direct, dict) or direct.get("pass") is not True:
            return "incomplete", "full profile lacks passing direct security regressions"
    return "pass", "the pinned profile passed"


def main():
    profile, directory = sys.argv[1:]
    if profile not in ("smoke", "full"):
        raise ValueError("profile must be smoke or full")
    artifacts = Path(directory)
    artifacts.mkdir(parents=True, exist_ok=True)
    status, reason = profile_status(profile, artifacts)
    (artifacts / "ci-status.json").write_text(json.dumps({"profile": profile, "status": status, "reason": reason}, indent=2) + "\n")
    summary = f"sqlmap / {profile}: {status.upper()}\n\n{reason}\n"
    (artifacts / "ci-summary.txt").write_text(summary)
    if destination := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(destination, "a") as output:
            output.write(summary)
    print(summary)
    return 0 if status == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())
