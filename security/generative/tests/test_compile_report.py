import unittest

from pgorm_campaign import compile_report, matrix
from pgorm_campaign.matrix import obligations


def _result(identity, obligation, verdict, status, **extra):
    return {
        "id": identity,
        "obligation": obligation,
        "verdict": verdict,
        "status": status,
        "detail": "",
        "codes": [],
        **extra,
    }


def _batch(name="b", *, unattributed=()):
    return {
        "name": name,
        "crate": "pgorm-compile-abc",
        "verdict": "reject",
        "phase": "typeck",
        "exit_code": 1,
        "seconds": 1.0,
        "toolchain_failure": False,
        "unattributed": list(unattributed),
        "results": [],
    }


CLEAN = [
    _result("p", "new-rust-entities", "accept", "compiled"),
    _result(
        "n",
        "rust-ownership",
        "reject",
        "expected-rejection",
        codes=["E0499"],
        coded=True,
    ),
    _result("u", "rust-ownership", "reject", "expected-rejection", coded=False),
    _result("g", "fresh-graph-generics", "accept", "compiled"),
    _result("r", "new-rust-entities", "refuse", "expected-refusal"),
]

COVERAGE = {
    "new-rust-entities": {"cases": 2, "satisfied": 2, "discharged": True},
    "rust-ownership": {"cases": 2, "satisfied": 2, "discharged": True},
    "fresh-graph-generics": {"cases": 1, "satisfied": 1, "discharged": True},
}


# [spec:pgorm:req:generative.compile-suite/test]
class ReportTests(unittest.TestCase):
    def test_counts_are_labelled_apart_from_runtime(self):
        document = compile_report.assemble(CLEAN, COVERAGE, [_batch()], seconds=12.0)
        self.assertEqual(document["kind"], "compile-coverage")
        self.assertEqual(document["compile_cases"], 5)
        self.assertEqual(document["compile_positive"], 2)
        self.assertEqual(document["compile_positive_compiled"], 2)
        self.assertEqual(document["compile_negative"], 2)
        self.assertEqual(document["compile_negative_rejected"], 2)
        self.assertEqual(document["compile_negative_rejected_by_code"], 1)
        self.assertEqual(document["compile_generator_refused"], 1)
        self.assertTrue(document["passed"])
        runtime = [key for key in document if key.startswith("runtime")]
        self.assertEqual(runtime, [])

    def test_defects_and_faults_are_separate_lists(self):
        results = CLEAN + [
            _result("d", "rust-ownership", "reject", "unrejected"),
            _result("f", "new-rust-entities", "accept", "toolchain-failure"),
        ]
        document = compile_report.assemble(results, COVERAGE, [_batch()], seconds=1.0)
        self.assertEqual([item["id"] for item in document["defects"]], ["d"])
        self.assertEqual([item["id"] for item in document["faults"]], ["f"])
        self.assertFalse(document["passed"])

    def test_unattributed_diagnostics_fail_the_run(self):
        document = compile_report.assemble(
            CLEAN,
            COVERAGE,
            [_batch(unattributed=[{"code": "E0463", "message": "no crate"}])],
            seconds=1.0,
        )
        self.assertEqual(document["unattributed"], 1)
        self.assertFalse(document["passed"])

    def test_open_obligation_fails_the_run(self):
        coverage = dict(COVERAGE)
        coverage["rust-ownership"] = {"cases": 2, "satisfied": 1, "discharged": False}
        document = compile_report.assemble(CLEAN, coverage, [_batch()], seconds=1.0)
        self.assertFalse(document["passed"])
        self.assertIn("OPEN", compile_report.render(document))

    def test_render_states_compile_coverage_explicitly(self):
        text = compile_report.render(
            compile_report.assemble(CLEAN, COVERAGE, [_batch()], seconds=3.25)
        )
        self.assertIn("separate from runtime counts", text)
        self.assertIn("by error code", text)
        self.assertIn("passed", text)


# [spec:pgorm:req:generative.compile-suite/test]
class SeparationTests(unittest.TestCase):
    def test_runtime_obligations_exclude_compile_only_ids(self):
        required = obligations()
        compile_only = [
            entry["id"]
            for entry in matrix.load()["outside_runtime"]
            if entry["status"] == "compile-only"
        ]
        for identity in compile_only:
            self.assertNotIn(identity, required)
            self.assertFalse(any(identity in item for item in required))


if __name__ == "__main__":
    unittest.main()
