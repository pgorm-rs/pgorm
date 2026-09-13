"""CI must not turn missing, partial or failing campaign evidence into a pass."""

import contextlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import ci  # noqa: E402

PASSING = {
    "passed": True,
    "profile": {"name": "full", "profile_version": 1},
    "counts": {
        "note": "run classes are counted separately and are never summed",
        "construction_only_programs_constructed": {"scheduled": 9, "recorded": 9},
        "live_database_programs_checked": {"scheduled": 5, "recorded": 5},
        "compile_suite_invocations": {"scheduled": 2, "recorded": 2},
    },
    "compile": {"included": True, "available": True, "compile_cases": 2},
    "coverage": {
        "obligations": "full-matrix",
        "declared_satisfied": 3,
        "declared_total": 3,
        "full_matrix": {"outstanding": 0},
    },
}


def _run(artifacts, name="full-0123456789ab"):
    directory = artifacts / "run" / name
    directory.mkdir(parents=True)
    return directory


# [spec:pgorm:req:generative.ci/test]
class ProfileStatusTests(unittest.TestCase):
    def test_missing_report_is_explicitly_not_run(self):
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory)
            self.assertEqual(ci.profile_status("smoke", artifacts)[0], "not-run")
            (artifacts / "run").mkdir()
            self.assertEqual(ci.profile_status("smoke", artifacts)[0], "not-run")

    def test_a_killed_campaign_reports_failure_not_absence(self):
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory)
            run = _run(artifacts)
            self.assertEqual(ci.profile_status("full", artifacts)[0], "fail")
            (run / "summary.json").write_text(json.dumps({"passed": False}))
            self.assertEqual(ci.profile_status("full", artifacts)[0], "fail")

    def test_ambiguous_run_directories_are_incomplete(self):
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory)
            _run(artifacts)
            _run(artifacts, "full-ba9876543210")
            self.assertEqual(ci.profile_status("full", artifacts)[0], "incomplete")

    def test_profile_status_requires_matching_complete_evidence(self):
        cases = (
            ({}, "pass"),
            ({"passed": False}, "fail"),
            ({"passed": None}, "fail"),
            ({"profile": {"name": "smoke", "profile_version": 1}}, "incomplete"),
            ({"profile": "full"}, "pass"),
            ({"coverage": {"obligations": "full-matrix"}}, "incomplete"),
            ({"coverage": None}, "incomplete"),
        )
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory)
            path = _run(artifacts) / "campaign.json"
            for update, expected in cases:
                with self.subTest(update=update):
                    path.write_text(json.dumps(PASSING | update))
                    self.assertEqual(ci.profile_status("full", artifacts)[0], expected)

    def test_summed_or_hidden_compile_counts_are_incomplete(self):
        summed = PASSING["counts"] | {"total": 16}
        hidden = {
            key: value
            for key, value in PASSING["counts"].items()
            if key != "compile_suite_invocations"
        }
        cases = (
            ({"counts": summed}, "incomplete"),
            ({"counts": hidden}, "incomplete"),
            ({"counts": None}, "incomplete"),
            ({"compile": {"included": True, "available": False}}, "incomplete"),
            ({"compile": {"available": True}}, "incomplete"),
            ({"compile": {"included": False, "reason": "excluded"}}, "incomplete"),
        )
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory)
            path = _run(artifacts) / "campaign.json"
            for update, expected in cases:
                with self.subTest(update=update):
                    path.write_text(json.dumps(PASSING | update))
                    self.assertEqual(ci.profile_status("full", artifacts)[0], expected)

    def test_an_excluded_compile_suite_still_passes_smoke(self):
        report = PASSING | {
            "profile": {"name": "smoke", "profile_version": 1},
            "compile": {"included": False, "reason": "scheduled by the full profile"},
        }
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory)
            path = _run(artifacts, "smoke-0123456789ab") / "campaign.json"
            path.write_text(json.dumps(report))
            self.assertEqual(ci.profile_status("smoke", artifacts)[0], "pass")

    def test_malformed_report_documents_are_incomplete(self):
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory)
            path = _run(artifacts) / "campaign.json"
            for malformed in ("{", "[]", "null", '"full"', ""):
                with self.subTest(malformed=malformed):
                    path.write_text(malformed)
                    self.assertEqual(
                        ci.profile_status("full", artifacts)[0], "incomplete"
                    )


def _publish(profile, artifacts, step=None):
    """Drive the command the workflow runs, without its output joining ours."""
    argv, environment = sys.argv, dict(os.environ)
    sys.argv = ["ci.py", profile, str(artifacts)]
    if step:
        os.environ["GITHUB_STEP_SUMMARY"] = str(step)
    else:
        os.environ.pop("GITHUB_STEP_SUMMARY", None)
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            return ci.main()
    finally:
        sys.argv = argv
        os.environ.clear()
        os.environ.update(environment)


# [spec:pgorm:req:generative.ci/test]
class PublishedStatusTests(unittest.TestCase):
    def test_the_published_status_reaches_the_step_summary(self):
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory)
            step = artifacts / "step-summary.txt"
            (_run(artifacts) / "campaign.json").write_text(json.dumps(PASSING))
            self.assertEqual(_publish("full", artifacts, step), 0)
            published = json.loads((artifacts / "ci-status.json").read_text())
            self.assertEqual(published["status"], "pass")
            self.assertIn("FULL: PASS", step.read_text().upper())

    def test_a_failing_campaign_exits_non_zero(self):
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory)
            path = _run(artifacts) / "campaign.json"
            path.write_text(json.dumps(PASSING | {"passed": False}))
            self.assertEqual(_publish("full", artifacts), 1)
            self.assertIn("FAIL", (artifacts / "ci-summary.txt").read_text())

    def test_a_never_run_campaign_exits_non_zero(self):
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory) / "absent"
            self.assertEqual(_publish("smoke", artifacts), 1)
            published = json.loads((artifacts / "ci-status.json").read_text())
            self.assertEqual(published["status"], "not-run")
            self.assertIsNone(published["run"])


if __name__ == "__main__":
    unittest.main()
