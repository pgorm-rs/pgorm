import json
import unittest

from pgorm_campaign import compile_crate, compile_driver, compile_runner
from pgorm_campaign.compile_case import CompileCase, rejects

MANIFEST = "/tmp/batch/Cargo.toml"


def _case(identity="a", **overrides):
    fields = {
        "id": identity,
        "obligation": "rust-ownership",
        "verdict": "accept",
        "phase": "typeck",
        "source": "    pub fn f() {}",
    }
    fields.update(overrides)
    return CompileCase(**fields)


def _message(code, text, file_name, line, *, manifest=MANIFEST):
    return json.dumps(
        {
            "reason": "compiler-message",
            "manifest_path": manifest,
            "message": {
                "level": "error",
                "code": {"code": code} if code else None,
                "message": text,
                "spans": [
                    {"is_primary": True, "file_name": file_name, "line_start": line}
                ],
            },
        }
    )


def _built(diagnostics, *, exit_code=1, stderr=""):
    return {
        "crate": "pgorm-compile-abc",
        "manifest": MANIFEST,
        "exit_code": exit_code,
        "succeeded": exit_code == 0,
        "stderr": stderr,
        "diagnostics": diagnostics,
        "seconds": 0.5,
    }


def _rejecting_batch():
    cases = [
        _case("first", verdict="reject", expects=rejects("E0499")),
        _case("second", verdict="reject", expects=rejects("E0277")),
    ]
    batch = compile_crate.group(cases)[0]
    compile_crate.render_source(batch)
    return batch


# [spec:pgorm:req:generative.compile-suite/test]
class DiagnosticTests(unittest.TestCase):
    def test_only_this_crate_errors_are_collected(self):
        stdout = "\n".join(
            [
                _message("E0499", "two borrows", "src/lib.rs", 12),
                _message("E0277", "elsewhere", "src/lib.rs", 4, manifest="/other.toml"),
                json.dumps(
                    {
                        "reason": "compiler-message",
                        "manifest_path": MANIFEST,
                        "message": {
                            "level": "warning",
                            "message": "unused",
                            "spans": [],
                        },
                    }
                ),
                json.dumps({"reason": "build-finished", "success": False}),
                "not json at all",
            ]
        )
        found, finished = compile_runner._diagnostics(stdout, MANIFEST)
        self.assertEqual([item["code"] for item in found], ["E0499"])
        self.assertIs(finished, False)

    def test_uncoded_diagnostics_keep_their_message(self):
        stdout = _message("", "lifetime may not live long enough", "src/lib.rs", 3)
        found, _ = compile_runner._diagnostics(stdout, MANIFEST)
        self.assertEqual(found[0]["code"], "")
        self.assertIn("lifetime", found[0]["message"])


# [spec:pgorm:req:generative.compile-suite/test]
class VerdictTests(unittest.TestCase):
    def test_silence_never_passes_a_negative(self):
        batch = _rejecting_batch()
        scored = compile_runner.score(batch, _built([], exit_code=0))
        self.assertEqual(
            [item["status"] for item in scored["results"]],
            ["unrejected", "unrejected"],
        )

    def test_negative_passes_only_on_its_own_code(self):
        batch = _rejecting_batch()
        first, second = batch.placements
        diagnostics, _ = compile_runner._diagnostics(
            "\n".join(
                [
                    _message("E0499", "two borrows", "src/lib.rs", first.start),
                    _message("E0308", "mismatched", "src/lib.rs", second.start),
                ]
            ),
            MANIFEST,
        )
        scored = compile_runner.score(batch, _built(diagnostics))
        statuses = {item["id"]: item["status"] for item in scored["results"]}
        self.assertEqual(statuses["first"], "expected-rejection")
        self.assertEqual(statuses["second"], "misrejected")

    def test_another_cases_rejection_does_not_count(self):
        batch = _rejecting_batch()
        first, _second = batch.placements
        diagnostics, _ = compile_runner._diagnostics(
            _message("E0277", "not satisfied", "src/lib.rs", first.start), MANIFEST
        )
        scored = compile_runner.score(batch, _built(diagnostics))
        statuses = {item["id"]: item["status"] for item in scored["results"]}
        self.assertEqual(statuses["first"], "misrejected")
        self.assertEqual(statuses["second"], "unrejected")

    def test_positive_batch_reports_its_own_failures(self):
        batch = compile_crate.group([_case("good"), _case("bad")])[0]
        compile_crate.render_source(batch)
        diagnostics, _ = compile_runner._diagnostics(
            _message("E0425", "not found", "src/lib.rs", batch.placements[1].start),
            MANIFEST,
        )
        scored = compile_runner.score(batch, _built(diagnostics))
        statuses = {item["id"]: item["status"] for item in scored["results"]}
        self.assertEqual(statuses["good"], "compiled")
        self.assertEqual(statuses["bad"], "unexpected-rejection")

    def test_toolchain_failure_is_not_a_rejection(self):
        batch = _rejecting_batch()
        scored = compile_runner.score(
            batch, _built([], exit_code=101, stderr="error: failed to select a version")
        )
        self.assertTrue(scored["toolchain_failure"])
        self.assertEqual(
            {item["status"] for item in scored["results"]}, {"toolchain-failure"}
        )

    def test_unattributed_diagnostics_are_kept_apart(self):
        batch = _rejecting_batch()
        diagnostics, _ = compile_runner._diagnostics(
            _message("E0463", "can't find crate", "", 0), MANIFEST
        )
        scored = compile_runner.score(batch, _built(diagnostics))
        self.assertEqual(len(scored["unattributed"]), 1)
        self.assertEqual({item["status"] for item in scored["results"]}, {"unrejected"})


# [spec:pgorm:req:generative.compile-suite/test]
class DriverClassificationTests(unittest.TestCase):
    def _codegen(self, identity, verdict, expects=None):
        return CompileCase(
            id=identity,
            obligation="new-rust-entities",
            verdict=verdict,
            phase="typeck",
            kind="codegen",
            request={"id": identity, "sql": "CREATE TABLE t (id serial PRIMARY KEY);"},
            expects=expects,
        )

    def test_generator_refusal_is_not_a_compile_rejection(self):
        case = self._codegen("r", "refuse", rejects(message="not valid Rust token"))
        verdict = compile_driver.classify(
            case,
            {
                "outcome": "generator-error",
                "message": "`x` entry is not valid Rust token text",
            },
        )
        self.assertEqual(verdict["status"], "expected-refusal")

    def test_refusal_must_say_what_was_predicted(self):
        case = self._codegen("r", "refuse", rejects(message="unsupported DDL"))
        verdict = compile_driver.classify(
            case, {"outcome": "generator-error", "message": "something else entirely"}
        )
        self.assertEqual(verdict["status"], "misrefused")

    def test_generating_where_refusal_expected_is_a_defect(self):
        case = self._codegen("r", "refuse", rejects(message="unsupported DDL"))
        verdict = compile_driver.classify(case, {"outcome": "generated", "files": []})
        self.assertEqual(verdict["status"], "unrefused")

    def test_generator_failure_is_not_a_compile_result(self):
        case = self._codegen("g", "accept")
        verdict = compile_driver.classify(
            case, {"outcome": "generator-error", "message": "boom"}
        )
        self.assertEqual(verdict["status"], "generator-failed")
        self.assertEqual(
            compile_driver.classify(case, {"outcome": "generated", "files": []})[
                "status"
            ],
            "generated",
        )


if __name__ == "__main__":
    unittest.main()
