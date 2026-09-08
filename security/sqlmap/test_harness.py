"""Fast negative tests for the scanner's completion and verdict boundaries."""
import copy
import unittest
from pathlib import Path

import harness as h

TARGET = "http://127.0.0.1:1234/case/protected/select?input=alice"
LOG = "GET parameter 'input' does not seem to be injectable\n[*] ending @ 12:00:00"


def clean_report():
    return {"success": True, "meta": {"url": TARGET}, "data": [], "error": [h.CLEAN_MESSAGE]}


def detection(technique="B", parameter="input", place="GET"):
    return {"complete": True, "findings": [{"parameter": parameter, "place": place, "data": [{"technique": h.TECHNIQUES[technique], "payload": "input=alice' AND 1=1 --"}]}]}


# [spec:pgorm:req:security.sqlmap.execution/test]
# [spec:pgorm:req:security.sqlmap.runner-tests]
# [spec:pgorm:req:security.sqlmap.runner-tests/test]
class ScannerResultTests(unittest.TestCase):
    def test_captured_sqlmap_output_and_validation_failures(self):
        fixtures = Path(__file__).parent / "adapter/tests/fixtures"
        for mode in ("control", "protected"):
            report = h.read_json(fixtures / f"{mode}.json")
            log = (fixtures / f"{mode}.log").read_text()
            target = report["meta"]["url"] + "?input=alice"
            result = h.interpret(report, log, 0, target, 25)
            self.assertTrue(result["complete"], result)
            for field, stale in (("scheme", target.replace("http:", "https:")), ("hostname", target.replace("127.0.0.1", "localhost")), ("port", target.replace(":56392", ":56393")), ("path", target.replace("/select", "/delete"))):
                failed = h.interpret(report, log, 0, stale, 25)
                self.assertFalse(failed["complete"])
                self.assertIn(field, failed["reason"])
                self.assertEqual(failed["findings"], result["findings"])
            failed = h.interpret(report, log, 0, target, 0)
            self.assertIn("inactive route", failed["reason"])
            self.assertEqual(failed["findings"], result["findings"])
            failed = h.interpret(report, log, 1, target, 25)
            self.assertIn("exited 1", failed["reason"])
            self.assertEqual(failed["findings"], result["findings"])

    def test_wrong_injection_parameter_preserves_finding_but_is_incomplete(self):
        report = clean_report()
        report["data"] = [{"type_name":"TECHNIQUES", "value":detection(parameter="other")["findings"]}]
        result = h.interpret(report, LOG, 0, TARGET, 25)
        self.assertFalse(result["complete"])
        self.assertIn("injection parameter", result["reason"])
        self.assertTrue(result["findings"])

    def test_complete_clean_scan(self):
        result = h.interpret(clean_report(), LOG, 0, TARGET, 25)
        self.assertTrue(result["complete"])
        self.assertEqual(result["findings"], [])

    def test_zero_exit_without_output_is_incomplete(self):
        for report in (None, {}, [], {"success": False}, {"success": True}):
            with self.subTest(report=report):
                self.assertFalse(h.interpret(report, LOG, 0, TARGET, 25)["complete"])

    def test_crash_cancel_timeout_are_incomplete(self):
        for returncode in (1, -9, -15, None):
            with self.subTest(returncode=returncode):
                self.assertFalse(h.interpret(clean_report(), LOG, returncode, TARGET, 25)["complete"])

    def test_inactive_route_is_incomplete(self):
        self.assertFalse(h.interpret(clean_report(), LOG, 0, TARGET, 0)["complete"])

    def test_previous_target_output_cannot_pass(self):
        report = clean_report()
        report["meta"]["url"] = TARGET.replace("select", "delete")
        self.assertFalse(h.interpret(report, LOG, 0, TARGET, 25)["complete"])

    def test_missing_terminal_evidence_cannot_pass(self):
        for log in ("", "[*] ending @", "parameter 'input' does not seem to be injectable"):
            self.assertFalse(h.interpret(clean_report(), log, 0, TARGET, 25)["complete"])

    def test_transport_or_skipped_scan_cannot_pass(self):
        for message in ("connection timed out", "unable to connect", "connection reset", "user aborted", "skipping parameter"):
            self.assertFalse(h.interpret(clean_report(), LOG+message, 0, TARGET, 25)["complete"])

    def test_scanner_error_is_not_a_clean_negative(self):
        report = clean_report()
        report["error"].append("unexpected internal exception")
        self.assertFalse(h.interpret(report, LOG, 0, TARGET, 25)["complete"])

    def test_named_techniques_are_preserved_for_replay(self):
        report = clean_report()
        report["error"] = []
        report["data"] = [{"type_name":"TECHNIQUES","value":detection()["findings"]}]
        result = h.interpret(report, "[*] ending @", 0, TARGET, 25)
        self.assertTrue(result["complete"])
        self.assertEqual(result["findings"], detection()["findings"])


# [spec:pgorm:req:security.sqlmap.outcomes/test]
# [spec:pgorm:req:security.sqlmap.controls/test]
# [spec:pgorm:req:security.sqlmap.verdict/test]
class VerdictTests(unittest.TestCase):
    def setUp(self):
        self.clean = {"complete": True, "findings": []}

    def test_detected_control_and_complete_protected_pass(self):
        self.assertEqual(h.verdict(detection(),self.clean,"B"),"pass")

    def test_undetected_control_fails(self):
        self.assertEqual(h.verdict(self.clean,self.clean,"B"),"invalid-control")

    def test_wrong_technique_or_injection_point_fails(self):
        for control in (detection("E"),detection(parameter="unrelated"),detection(place="Cookie")):
            self.assertEqual(h.verdict(control,self.clean,"B"),"invalid-control")

    def test_failed_baseline_and_unreachable_scan_are_incomplete(self):
        self.assertEqual(h.verdict(detection(),self.clean,"B",baseline=False),"incomplete")
        self.assertEqual(h.verdict(detection(),{"complete":False},"B"),"incomplete")
        self.assertEqual(h.verdict({"complete":False},self.clean,"B"),"incomplete")

    def test_protected_finding_and_sentinel_violation_fail(self):
        self.assertEqual(h.verdict(detection(),detection(),"B"),"vulnerable")
        self.assertEqual(h.verdict(detection(),self.clean,"B",invariant=False),"vulnerable")
        self.assertEqual(h.verdict({"complete":False},detection(),"B"),"vulnerable")

    def test_empty_discovery_missing_extra_duplicate_skipped_cases_fail(self):
        self.assertFalse(h.aggregate([],{},[]))
        self.assertFalse(h.aggregate(["a"],{},[]))
        self.assertFalse(h.aggregate(["a"],{"a":{"outcome":"pass"},"b":{"outcome":"pass"}},[]))
        self.assertFalse(h.aggregate(["a","a"],{"a":{"outcome":"pass"}},[]))
        for outcome in ("incomplete","invalid-control","vulnerable","skipped"):
            self.assertFalse(h.aggregate(["a"],{"a":{"outcome":outcome}},[]))

    def test_cleanup_failure_fails_complete_scan(self):
        self.assertFalse(h.aggregate(["a"],{"a":{"outcome":"pass"}},["container removal failed"]))

    def test_fully_accounted_clean_run_passes(self):
        self.assertTrue(h.aggregate(["a","b"],{"a":{"outcome":"pass"},"b":{"outcome":"pass"}},[]))


if __name__ == "__main__":
    unittest.main()
