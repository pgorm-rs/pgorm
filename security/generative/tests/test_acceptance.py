"""Acceptance must not read as complete when a component or obligation is missing."""

from pathlib import Path
import tempfile
import unittest
import unittest.mock

from pgorm_campaign import acceptance, acceptance_regressions

ROOT = Path(__file__).resolve().parents[3]

RUNTIME = {
    "passed": True,
    "profile": {"name": "full", "coverage": {"claim": "complete"}},
    "counts": {
        "note": "run classes are counted separately and are never summed",
        "construction_only_programs_constructed": {"recorded": 20000},
        "live_database_programs_checked": {"recorded": 5000},
    },
    "coverage": {
        "obligations": "full-matrix",
        "declared_satisfied": 430,
        "declared_total": 430,
        "full_matrix": {"outstanding": 0, "total": 430},
    },
    "controls": {"required": 22, "detected": 22},
    "findings": [],
    "timing": {"total_seconds": 100.0},
}

PARITY = {"passed": True, "families_declared": 15, "families_in_parity": 15}
COMPILE = {"passed": True, "compile_cases": 73, "unattributed": 0}
STRESS = {
    "passed": True,
    "distinct_generated_programs_constructed": 1000001,
    "reached_postgresql": 0,
    "oracle_decided": 0,
    "construction_errors": {},
}
REGRESSION = {
    "test": "deduplicated_relations_still_combine",
    "executed": True,
    "passed": True,
    "reason": "passes in this checkout",
    "counterfactual": {"attempted": True, "failed": True},
}


def _named():
    return [
        {**REGRESSION, "test": regression["test"]}
        for item in acceptance.resolved()
        for regression in item["regressions"]
    ]


def _assemble(**extra):
    fields = {
        "runtime": RUNTIME,
        "parity": PARITY,
        "compile_document": COMPILE,
        "stress": STRESS,
        "regressions": _named(),
        "directories": list(acceptance.FINDINGS),
    }
    fields.update(extra)
    return acceptance.assemble(**fields)


# [spec:pgorm:req:generative.acceptance/test]
class ComponentTests(unittest.TestCase):
    def test_every_component_ran_still_leaves_findings(self):
        document = _assemble()
        self.assertTrue(document["passed"])
        # Every component produced evidence, and acceptance is still not
        # complete: the open findings are outstanding and stay that way.
        self.assertFalse(document["complete"])
        self.assertTrue(
            all(item.startswith("open finding:") for item in document["outstanding"])
        )

    def test_no_open_findings_makes_it_complete(self):
        registry = {
            name: entry
            for name, entry in acceptance.FINDINGS.items()
            if entry["state"] == "resolved"
        }
        with unittest.mock.patch.object(acceptance, "FINDINGS", registry):
            document = _assemble(directories=list(registry))
        self.assertEqual(document["outstanding"], [])
        self.assertTrue(document["complete"])

    def test_an_absent_component_is_not_a_pass(self):
        for name in ("runtime", "parity", "compile_document", "stress"):
            document = _assemble(**{name: None})
            self.assertFalse(document["passed"], name)
            self.assertTrue(
                any("was supplied" in item for item in document["outstanding"]), name
            )

    def test_unrun_regressions_are_recorded_as_unrun(self):
        document = _assemble(regressions=[])
        self.assertFalse(document["components"]["regressions"]["executed"])
        self.assertFalse(document["passed"])

    def test_a_failing_component_fails_acceptance(self):
        document = _assemble(compile_document={**COMPILE, "passed": False})
        self.assertFalse(document["passed"])
        self.assertIn("compile: the component did not pass", document["outstanding"])


# [spec:pgorm:req:generative.acceptance/test]
class HonestyTests(unittest.TestCase):
    def test_smoke_coverage_leaves_obligations_outstanding(self):
        runtime = {
            **RUNTIME,
            "coverage": {
                "obligations": "scheduled-families",
                "declared_satisfied": 15,
                "declared_total": 15,
                "full_matrix": {"outstanding": 307, "total": 430},
            },
        }
        document = _assemble(runtime=runtime)
        joined = " ".join(document["outstanding"])
        self.assertIn("not the full matrix", joined)
        self.assertIn("307 of 430", joined)
        self.assertFalse(document["complete"])

    def test_open_findings_are_always_listed(self):
        document = _assemble()
        names = {item["finding"] for item in document["open_findings"]}
        self.assertIn("set-precedence", names)
        self.assertIn("pipeline-hidden-order", names)
        self.assertIn("pipeline-renamed-column-order", names)

    def test_diverging_parity_families_are_named(self):
        document = _assemble(
            parity={**PARITY, "passed": False, "families_diverging": ["pipeline"]}
        )
        self.assertTrue(
            any("diverges for pipeline" in x for x in document["outstanding"])
        )
        self.assertNotIn("nothing outstanding", acceptance.render(document))

    def test_retained_runtime_findings_are_named(self):
        runtime = {**RUNTIME, "passed": False, "findings": [{"item": "runtime-101"}]}
        document = _assemble(runtime=runtime)
        joined = " ".join(document["outstanding"])
        self.assertIn("retained 1 findings", joined)
        self.assertIn("runtime-101", joined)

    def test_an_attempted_profile_that_failed_shows(self):
        attempt = {
            "profile": "smoke",
            "completed": False,
            "reason": "fixture contention",
        }
        document = _assemble(attempts=[attempt])
        self.assertTrue(
            any("did not complete" in x for x in document["outstanding"]),
        )
        self.assertIn("fixture contention", acceptance.render(document))

    def test_stress_construction_errors_stay_visible(self):
        document = _assemble(
            stress={**STRESS, "construction_errors": {"FormatError": 42}}
        )
        self.assertTrue(
            any("42 programs failed construction" in x for x in document["outstanding"])
        )

    def test_an_unregistered_finding_fails_acceptance(self):
        document = _assemble(directories=[*acceptance.FINDINGS, "brand-new"])
        self.assertFalse(document["passed"])
        self.assertEqual(document["finding_registry"]["unregistered"], ["brand-new"])

    def test_a_vanished_finding_fails_acceptance(self):
        document = _assemble(directories=["set-precedence"])
        self.assertFalse(document["passed"])
        self.assertIn("distinct-append", document["finding_registry"]["missing"])

    def test_the_disclaimers_are_part_of_the_document(self):
        document = _assemble()
        joined = " ".join(document["disclaimers"])
        self.assertIn("does not replace full live coverage", joined)
        self.assertIn("all ORM programs are safe", joined)


# [spec:pgorm:req:generative.acceptance/test]
class RegistryTests(unittest.TestCase):
    def test_each_resolved_finding_names_a_regression(self):
        for item in acceptance.resolved():
            self.assertTrue(item["regressions"], item["finding"])
            for regression in item["regressions"]:
                self.assertTrue(regression["module"].endswith(regression["test"]))
                self.assertTrue(regression["path"].endswith(".rs"))

    def test_resolved_and_open_findings_do_not_overlap(self):
        resolved = {item["finding"] for item in acceptance.resolved()}
        self.assertFalse(resolved & {item["finding"] for item in acceptance.opened()})

    def test_the_rendered_report_names_both_traces(self):
        text = acceptance.render(_assemble())
        self.assertIn("pipeline-append-distinct", text)
        self.assertIn("deduplicated_relations_still_combine", text)
        self.assertIn("src/pipeline/tests.rs", text)
        self.assertIn("## Open findings", text)


# [spec:pgorm:req:generative.acceptance/test]
class RemovalTests(unittest.IsolatedAsyncioTestCase):
    """A counterfactual has to find its fix in the manifests as they are now.

    The removal for the prqlc fork once truncated the root manifest at a
    `[patch.crates-io]` header. When the fork became a dependency that header
    went away, and the removal would have reported its marker missing on
    every acceptance run rather than taking the fork out. So each dependency
    swap is applied here to copies of the checkout's own manifests.
    """

    async def test_each_dependency_swap_rewrites_only_its_declaration(self):
        swaps = {
            test: removal
            for test, removal in acceptance_regressions.REMOVALS.items()
            if removal["kind"] == "swap-dependency"
        }
        self.assertTrue(swaps, "the prqlc fork's counterfactual is a swap")
        for test, removal in swaps.items():
            with tempfile.TemporaryDirectory() as scratch:
                scratch = Path(scratch)
                before = {}
                for path in removal["paths"]:
                    (scratch / path).parent.mkdir(parents=True, exist_ok=True)
                    before[path] = (ROOT / path).read_text()
                    (scratch / path).write_text(before[path])
                await acceptance_regressions._remove(scratch, removal)
                key = removal["dependency"] + " = "
                for path in removal["paths"]:
                    old = before[path].splitlines()
                    new = (scratch / path).read_text().splitlines()
                    changed = [
                        (was, now)
                        for was, now in zip(old, new, strict=True)
                        if was != now
                    ]
                    self.assertEqual(
                        changed,
                        [
                            (was, key + removal["declaration"])
                            for was in old
                            if was.startswith(key)
                        ],
                        f"{test}: {path}",
                    )
                    self.assertEqual(len(changed), 1, f"{test}: {path}")
                    self.assertIn("git = ", changed[0][0], f"{test}: {path}")


if __name__ == "__main__":
    unittest.main()
