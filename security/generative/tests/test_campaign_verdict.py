"""Every item gets one of five verdicts, and a fault fails the whole run."""

import unittest

from pgorm_campaign import campaign_plan, campaign_verdict as verdict, profiles


def record(identity, run_class="runtime", **fields):
    value = {
        "id": identity,
        "run_class": run_class,
        "worker": 0 if run_class in profiles.LIVE_CLASSES else None,
        "expected": "pass",
        "verdict": "pass",
    }
    value.update(fields)
    return value


class Item:
    def __init__(self, identity, run_class="runtime", worker=0):
        self.id = identity
        self.run_class = run_class
        self.worker = worker
        self.family = None

    def data(self):
        return {"id": self.id, "run_class": self.run_class, "worker": self.worker}


class Plan:
    def __init__(self, items):
        self.items = tuple(items)

    def by_class(self, name):
        return tuple(item for item in self.items if item.run_class == name)

    def counts(self):
        return {
            name: len(self.by_class(name))
            for name in profiles.CLASSES
            if self.by_class(name)
        }


class VerdictMappingTest(unittest.TestCase):
    # [spec:pgorm:req:generative.verdict/test]
    def test_oracle_statuses_map_onto_recorded_verdicts(self):
        for status in ("pass", "defect", "expected-rejection", "incomplete"):
            self.assertEqual(verdict.from_checker({"status": status}), status)
        self.assertEqual(verdict.from_checker({}), "incomplete")
        self.assertEqual(
            verdict.from_checker({"status": "invalid-control"}), "incomplete"
        )

    # [spec:pgorm:req:generative.verdict/test]
    def test_control_outcomes_keep_their_own_verdict(self):
        self.assertEqual(verdict.from_control({"status": "valid-control"}), "pass")
        self.assertEqual(
            verdict.from_control({"status": "invalid-control"}), "invalid-control"
        )
        self.assertEqual(verdict.from_control({"status": "boom"}), "incomplete")

    # [spec:pgorm:req:generative.verdict/test]
    def test_construction_verdict_says_only_constructed(self):
        self.assertEqual(verdict.from_construction({"constructed": True}), "pass")
        self.assertEqual(
            verdict.from_construction({"constructed": False}), "incomplete"
        )

    # [spec:pgorm:req:generative.verdict/test]
    def test_compile_faults_are_incomplete_not_defects(self):
        self.assertEqual(verdict.from_compile({"passed": True}), "pass")
        self.assertEqual(verdict.from_compile({"defects": [1]}), "defect")
        self.assertEqual(
            verdict.from_compile({"faults": [1], "defects": [1]}), "incomplete"
        )
        self.assertEqual(verdict.from_compile(None), "incomplete")

    def test_verdict_vocabulary_stays_closed(self):
        self.assertEqual(len(verdict.VERDICTS), 5)
        with self.assertRaises(ValueError):
            verdict.fault("invented", "detail")


class WorkFaultTest(unittest.TestCase):
    # [spec:pgorm:req:generative.verdict/test]
    def test_empty_discovery_is_a_fault(self):
        self.assertEqual(
            verdict.work_faults(Plan([]), [])[0]["kind"], "empty-discovery"
        )
        plan = Plan([Item("a")])
        self.assertEqual(verdict.work_faults(plan, [])[0]["kind"], "empty-discovery")

    # [spec:pgorm:req:generative.verdict/test]
    def test_skipped_work_is_a_fault(self):
        plan = Plan([Item("a"), Item("b")])
        kinds = {f["kind"] for f in verdict.work_faults(plan, [record("a")])}
        self.assertIn("skipped-work", kinds)

    # [spec:pgorm:req:generative.verdict/test]
    def test_duplicate_records_are_a_fault(self):
        plan = Plan([Item("a")])
        kinds = {
            f["kind"] for f in verdict.work_faults(plan, [record("a"), record("a")])
        }
        self.assertIn("duplicate-record", kinds)

    # [spec:pgorm:req:generative.verdict/test]
    def test_unexpected_verdict_and_panic_are_faults(self):
        plan = Plan([Item("a"), Item("b")])
        faults = verdict.work_faults(
            plan,
            [
                record("a", verdict="defect"),
                record(
                    "b",
                    verdict="incomplete",
                    error={"class": "UnexpectedNativePanic", "cause": "panic"},
                ),
            ],
        )
        kinds = {f["kind"] for f in faults}
        self.assertIn("unexpected-verdict", kinds)
        self.assertIn("native-panic", kinds)

    # [spec:pgorm:req:generative.verdict/test]
    def test_cleanup_and_deadline_and_builds_are_faults(self):
        plan = Plan([Item("a"), Item("b"), Item("c")])
        faults = verdict.work_faults(
            plan,
            [
                record("a", cleanup_errors=["pool leak"]),
                record(
                    "b",
                    verdict="incomplete",
                    expected="incomplete",
                    deadline_exceeded=True,
                ),
                record("c", builds=1),
            ],
        )
        kinds = {f["kind"] for f in faults}
        self.assertIn("cleanup-failure", kinds)
        self.assertIn("deadline", kinds)
        self.assertIn("unamortized-build", kinds)

    # [spec:pgorm:req:generative.verdict/test]
    def test_missing_verdict_is_never_silently_accepted(self):
        plan = Plan([Item("a")])
        faults = verdict.work_faults(plan, [record("a", verdict=None)])
        self.assertEqual(faults[0]["kind"], "unrecorded-verdict")

    # [spec:pgorm:req:generative.verdict/test]
    def test_missing_worker_fails_the_run(self):
        profile = profiles.select("full")
        faults = verdict.worker_faults(profile, [record("a", worker=0)])
        self.assertEqual({f["kind"] for f in faults}, {"missing-worker"})
        self.assertEqual(len(faults), 3)

    # [spec:pgorm:req:generative.verdict/test]
    def test_wall_clock_budgets_are_enforced(self):
        profile = profiles.select("smoke")
        faults = verdict.budget_faults(
            profile, {"class_seconds": {"runtime": 10**6}, "total_seconds": 10**7}
        )
        self.assertEqual({f["kind"] for f in faults}, {"deadline"})
        self.assertEqual(len(faults), 2)


class AggregateTest(unittest.TestCase):
    def setUp(self):
        self.profile = profiles.select("smoke")
        self.plan = campaign_plan.schedule(self.profile)

    def aggregate(self, **overrides):
        arguments = {
            "coverage": {"satisfied": True, "declared_missing": []},
            "controls": {"passed": True},
            "faults": [],
        }
        arguments.update(overrides)
        records = [
            {**item.data(), "verdict": item.expected, "expected": item.expected}
            for item in self.plan.items
        ]
        return verdict.aggregate(self.profile, self.plan, records, **arguments)

    # [spec:pgorm:req:generative.verdict/test]
    def test_aggregate_success_needs_every_conjunct(self):
        self.assertTrue(self.aggregate()["passed"])
        self.assertFalse(
            self.aggregate(coverage={"satisfied": False, "declared_missing": ["x"]})[
                "passed"
            ]
        )
        self.assertFalse(self.aggregate(controls={"passed": False})["passed"])
        self.assertFalse(
            self.aggregate(faults=[verdict.fault("deadline", "late")])["passed"]
        )

    # [spec:pgorm:req:generative.verdict/test]
    def test_counts_are_reported_per_run_class(self):
        tally = verdict.counts(
            [
                record("a", "runtime"),
                record("b", "construction"),
                record("c", "control"),
            ]
        )
        self.assertEqual(set(tally), {"runtime", "construction", "control"})
        self.assertEqual(tally["runtime"]["pass"], 1)
        self.assertEqual(tally["construction"]["defect"], 0)


if __name__ == "__main__":
    unittest.main()
