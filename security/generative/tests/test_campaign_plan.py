"""Scheduling produces countable, distinct work with no idle declared worker."""

import copy
import unittest

from pgorm_campaign import campaign_plan, profiles


def profile(name="smoke", **overrides):
    document, digest = profiles.load()
    value = copy.deepcopy(document["profiles"][name])
    value.update(overrides)
    return profiles.Profile(name, value, digest)


class Spec:
    def __init__(self, identity):
        self.id = identity


class SchedulingTest(unittest.TestCase):
    # [spec:pgorm:req:generative.profiles/test]
    def test_smoke_schedules_each_class_separately(self):
        plan = campaign_plan.schedule(profiles.select("smoke"))
        counts = plan.counts()
        self.assertEqual(counts["construction"], 150)
        self.assertEqual(counts["runtime"], 30)
        self.assertEqual(counts["invalid"], 6)
        self.assertEqual(counts["control"], 22)
        self.assertNotIn("compile", counts)
        self.assertEqual(len(plan.items), sum(counts.values()))

    # [spec:pgorm:req:generative.verdict/test]
    def test_scheduled_identities_are_distinct(self):
        plan = campaign_plan.schedule(profiles.select("smoke"))
        identities = [item.id for item in plan.items]
        self.assertEqual(len(set(identities)), len(identities))

    # [spec:pgorm:req:generative.profiles/test]
    def test_construction_items_never_take_a_worker(self):
        plan = campaign_plan.schedule(profiles.select("smoke"))
        self.assertTrue(
            all(item.worker is None for item in plan.by_class("construction"))
        )
        self.assertTrue(all(item.worker == 0 for item in plan.by_class("runtime")))

    # [spec:pgorm:req:generative.profiles/test]
    def test_invalid_items_declare_expected_rejection(self):
        plan = campaign_plan.schedule(profiles.select("smoke"))
        items = plan.by_class("invalid")
        self.assertTrue(all(item.mode == "invalid" for item in items))
        self.assertTrue(all(item.expected == "expected-rejection" for item in items))
        self.assertTrue(all(item.family == "rejection" for item in items))

    # [spec:pgorm:req:generative.profiles/test]
    def test_live_work_spreads_over_declared_workers(self):
        plan = campaign_plan.schedule(profiles.select("full"))
        workers = {item.worker for item in plan.by_class("runtime")}
        self.assertEqual(workers, {0, 1, 2, 3})
        self.assertEqual(len(plan.by_class("compile")), 1)

    # [spec:pgorm:req:generative.verdict/test]
    def test_idle_declared_worker_is_refused(self):
        document = copy.deepcopy(profiles.load()[0]["profiles"]["smoke"])
        document["workers"] = 4
        document["run_classes"]["runtime"]["programs"] = 1
        document["run_classes"]["invalid"]["included"] = False
        document["run_classes"]["invalid"]["reason"] = "narrowed for this check"
        document["run_classes"]["control"]["included"] = False
        document["run_classes"]["control"]["reason"] = "narrowed for this check"
        with self.assertRaises(campaign_plan.PlanError):
            campaign_plan.schedule(profiles.Profile("narrow", document, "x" * 64))

    # [spec:pgorm:req:generative.profiles/test]
    def test_program_count_cannot_exceed_the_cap(self):
        document = copy.deepcopy(profiles.load()[0]["profiles"]["smoke"])
        document["generation_limits"]["max_programs"] = 2
        with self.assertRaises(campaign_plan.PlanError):
            campaign_plan.schedule(profiles.Profile("capped", document, "x" * 64))

    # [spec:pgorm:req:generative.verdict/test]
    def test_short_control_catalog_is_refused(self):
        with self.assertRaises(campaign_plan.PlanError):
            campaign_plan.schedule(
                profiles.select("smoke"), controls=[Spec("only-one")]
            )

    def test_plan_data_reports_counts_per_class(self):
        plan = campaign_plan.schedule(profiles.select("smoke"))
        data = plan.data()
        self.assertEqual(data["scheduled_total"], len(plan.items))
        self.assertEqual(data["scheduled_by_class"], plan.counts())
        self.assertEqual(len(data["items"]), len(plan.items))


if __name__ == "__main__":
    unittest.main()
