"""CI must not turn missing, partial or failing runner evidence into a pass."""
import json
from pathlib import Path
import tempfile
import unittest

import ci


# [spec:pgorm:req:security.sqlmap.ci/test]
class ProfileStatusTests(unittest.TestCase):
    def test_missing_report_is_explicitly_not_run(self):
        with tempfile.TemporaryDirectory() as directory:
            self.assertEqual(ci.profile_status("smoke", Path(directory))[0], "not-run")

    def test_profile_status_requires_matching_complete_evidence(self):
        passing = {"profile": "full", "subset": [], "pass": True, "direct_regressions": {"pass": True}}
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory)
            (artifacts / "run").mkdir()
            path = artifacts / "run/report.json"
            for update, expected in (({}, "pass"), ({"pass": False}, "fail"), ({"subset": ["select"]}, "incomplete"), ({"profile": "smoke"}, "incomplete"), ({"direct_regressions": {"pass": False}}, "incomplete"), ({"direct_regressions": None}, "incomplete")):
                with self.subTest(update=update):
                    path.write_text(json.dumps(passing | update))
                    self.assertEqual(ci.profile_status("full", artifacts)[0], expected)
            for malformed in ("{", "[]", "null"):
                path.write_text(malformed)
                self.assertEqual(ci.profile_status("full", artifacts)[0], "incomplete")
