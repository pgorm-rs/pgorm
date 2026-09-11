"""Reduction preserves the recorded failure and never invents a smaller one."""

import asyncio
import unittest

from pgorm_campaign import shrink
from pgorm_campaign.grammar import generate
from pgorm_campaign.parameters import input_ids
from pgorm_campaign.program import Program


def defect(reason="row multisets differ", oracle="reference", observation=None):
    return {
        "status": "defect",
        "comparisons": [
            {"step": "s0", "oracle": oracle, "equal": False, "reason": reason},
            {"step": "final", "oracle": "fixture-state", "equal": True, "reason": "="},
        ],
        "subject": {
            "steps": [
                {
                    "id": "s0",
                    "observation": observation or {"kind": "rows", "rows": []},
                }
            ]
        },
    }


def rejected(name="DatabaseError", sqlstate="42703", cause="column does not exist"):
    return {"kind": "error", "class": name, "cause": cause, "sqlstate": sqlstate}


def passing():
    return {
        "status": "pass",
        "comparisons": [
            {"step": "s0", "oracle": "reference", "equal": True, "reason": "equal"}
        ],
    }


def incomplete(name="UnsupportedCapabilityError", cause="vector type absent"):
    return {
        "status": "incomplete",
        "comparisons": [],
        "error": {
            "class": name,
            "cause": cause,
        },
    }


class Recorder:
    """A checker stand-in: verdicts come from a rule over the program itself."""

    def __init__(self, rule):
        self.rule = rule
        self.seen = []

    async def run(self, program, *, timeout=10):
        self.seen.append(program)
        return self.rule(program)


def keeps(op):
    """Interesting exactly while the program still contains this instruction."""

    def rule(program):
        data = program.data()
        entries = data["nodes"] + data["steps"]
        if any(entry["op"] == op for entry in entries):
            return defect()
        return passing()

    return rule


def program_with(op, limit=40, family=None):
    for index in range(limit):
        candidate = generate(20260911, index, family=family).program
        data = candidate.data()
        if any(node["op"] == op for node in data["nodes"]):
            return candidate
        if any(step["op"] == op for step in data["steps"]):
            return candidate
    raise AssertionError("no generated program contains " + op)


# [spec:pgorm:req:generative.shrink/test]
class PredicateTests(unittest.TestCase):
    def test_a_passing_report_cannot_be_shrunk(self):
        with self.assertRaises(shrink.ShrinkError):
            shrink.Predicate.freeze(passing())

    def test_an_expected_rejection_cannot_be_shrunk(self):
        with self.assertRaises(shrink.ShrinkError):
            shrink.Predicate.freeze({"status": "expected-rejection"})

    def test_a_defect_freezes_its_first_difference(self):
        predicate = shrink.Predicate.freeze(defect())
        self.assertEqual(predicate.status, "defect")
        self.assertEqual(predicate.kind, "difference")
        self.assertEqual(
            predicate.data(),
            {
                "status": "defect",
                "kind": "difference",
                "oracle": "reference",
                "reason": "row multisets differ",
                "witness": ["observation", "rows"],
            },
        )

    def test_a_different_reason_is_another_defect(self):
        predicate = shrink.Predicate.freeze(defect())
        self.assertFalse(predicate.holds(defect("ordered rows differ")))
        self.assertFalse(predicate.holds(defect(oracle="exact-error")))
        self.assertTrue(predicate.holds(defect()))

    def test_an_unsupported_capability_never_replaces_a_defect(self):
        predicate = shrink.Predicate.freeze(defect())
        self.assertFalse(predicate.holds(incomplete()))
        self.assertFalse(predicate.holds(passing()))

    def test_an_incomplete_original_pins_its_exact_error(self):
        predicate = shrink.Predicate.freeze(incomplete("PanicException", "native"))
        self.assertTrue(predicate.holds(incomplete("PanicException", "native")))
        self.assertFalse(predicate.holds(incomplete("PanicException", "other")))
        self.assertFalse(predicate.holds(incomplete("FormatError", "native")))
        self.assertFalse(predicate.holds(defect()))

    def test_an_incomplete_report_needs_a_recorded_error(self):
        with self.assertRaises(shrink.ShrinkError):
            shrink.Predicate.freeze({"status": "incomplete", "comparisons": []})

    def test_a_construction_error_is_not_a_rejection(self):
        # Both report "observation categories differ"; only one is the defect
        # that was found. Reducing a PostgreSQL 42703 down to a query the
        # builder never compiled is a different bug, not a smaller one.
        original = defect("observation categories differ", observation=rejected())
        predicate = shrink.Predicate.freeze(original)
        self.assertEqual(predicate.witness, ("error", "DatabaseError", "42703"))
        self.assertTrue(predicate.holds(original))
        drifted = defect(
            "observation categories differ",
            observation=rejected("ConstructionError", None, "PRQL compilation failed"),
        )
        self.assertFalse(predicate.holds(drifted))

    def test_a_different_sqlstate_is_a_different_rejection(self):
        predicate = shrink.Predicate.freeze(
            defect("observation categories differ", observation=rejected())
        )
        self.assertFalse(
            predicate.holds(
                defect(
                    "observation categories differ",
                    observation=rejected(sqlstate="42P01"),
                )
            )
        )

    def test_the_same_rejection_survives_new_message_text(self):
        # Shrinking is entitled to change the values an error message quotes.
        predicate = shrink.Predicate.freeze(
            defect("observation categories differ", observation=rejected())
        )
        self.assertTrue(
            predicate.holds(
                defect(
                    "observation categories differ",
                    observation=rejected(cause='column "p_score" does not exist'),
                )
            )
        )


# [spec:pgorm:req:generative.shrink/test]
class CandidateTests(unittest.TestCase):
    def test_every_offered_candidate_is_a_valid_program(self):
        source = generate(20260911, 3).program
        offered = 0
        for _, reduction in shrink.REDUCTIONS:
            for _, rewritten in reduction(source.data()):
                candidate = shrink._candidate(rewritten)
                if candidate is None:
                    continue
                offered += 1
                Program(candidate.encoded)
        self.assertGreater(offered, 0)

    def test_candidates_keep_dependencies_and_scopes_intact(self):
        source = program_with("pipeline.filter")
        for _, reduction in shrink.REDUCTIONS:
            for _, rewritten in reduction(source.data()):
                candidate = shrink._candidate(rewritten)
                if candidate is None:
                    continue
                data = candidate.data()
                ids = {node["id"] for node in data["nodes"]}
                binders = {binder["id"] for binder in data["binders"]}
                owners = {binder["owner"] for binder in data["binders"]}
                self.assertLessEqual(owners, ids)
                for node in data["nodes"]:
                    self.assertLessEqual(set(input_ids(node["inputs"])), ids)
                    self.assertIn(node["scope"], binders | {"root"})
                observed = {item["step"] for item in data["observations"]}
                self.assertEqual(
                    observed, {step["id"] for step in data["steps"]} | {"final"}
                )

    def test_a_reduction_never_returns_the_program_unchanged(self):
        source = generate(20260911, 3).program
        for _, reduction in shrink.REDUCTIONS:
            for _, rewritten in reduction(source.data()):
                candidate = shrink._candidate(rewritten)
                if candidate is not None:
                    self.assertNotEqual(candidate.digest, source.digest)


# [spec:pgorm:req:generative.shrink/test]
class ReduceTests(unittest.IsolatedAsyncioTestCase):
    async def test_reduction_shrinks_while_preserving_the_predicate(self):
        source = program_with("pipeline.filter")
        checker = Recorder(keeps("pipeline.filter"))
        result = await shrink.reduce(checker, source)
        report = result.report()
        self.assertTrue(report["reduced"])
        self.assertLessEqual(report["best"]["nodes"], report["original"]["nodes"])
        self.assertTrue(
            any(node["op"] == "pipeline.filter" for node in result.best.data()["nodes"])
        )
        self.assertEqual(result.predicate.data()["reason"], "row multisets differ")

    async def test_every_candidate_runs_through_the_checker(self):
        source = generate(20260911, 3).program
        checker = Recorder(keeps("value"))
        result = await shrink.reduce(
            checker, source, budget=shrink.Budget(candidates=12)
        )
        # The baseline plus one run per considered candidate, and every one of
        # them is a fully validated Program the checker restores fixtures for.
        self.assertEqual(len(checker.seen), result.report()["executed"] + 1)
        for seen in checker.seen:
            self.assertIsInstance(seen, Program)

    async def test_a_program_that_stops_failing_is_dropped(self):
        source = generate(20260911, 3).program
        checker = Recorder(lambda program: passing())
        result = await shrink.reduce(
            checker, source, report=defect(), budget=shrink.Budget(candidates=6)
        )
        self.assertEqual(result.best.digest, source.digest)
        report = result.report()
        self.assertFalse(report["reduced"])
        self.assertEqual(report["accepted"], 0)
        self.assertTrue(all(i["outcome"] != "accepted" for i in report["attempts"]))

    async def test_an_incomplete_candidate_never_replaces_the_defect(self):
        source = generate(20260911, 3).program
        checker = Recorder(lambda program: incomplete())
        result = await shrink.reduce(
            checker, source, report=defect(), budget=shrink.Budget(candidates=6)
        )
        self.assertEqual(result.best.digest, source.digest)
        statuses = {i["status"] for i in result.report()["attempts"] if i["status"]}
        self.assertEqual(statuses, {"incomplete"})

    async def test_budget_exhaustion_is_recorded_not_hidden(self):
        source = program_with("pipeline.filter")
        checker = Recorder(keeps("pipeline.filter"))
        budget = shrink.Budget(candidates=3)
        result = await shrink.reduce(checker, source, budget=budget)
        report = result.report()
        self.assertEqual(report["exhausted"], "candidate budget")
        self.assertLessEqual(report["executed"], 3)
        self.assertEqual(report["budget"]["candidates"], 3)

    async def test_interruption_keeps_the_original_and_best(self):
        source = program_with("pipeline.filter")
        calls = {"n": 0}

        def rule(program):
            calls["n"] += 1
            if calls["n"] > 4:
                raise KeyboardInterrupt
            return keeps("pipeline.filter")(program)

        result = await shrink.reduce(Recorder(rule), source)
        report = result.report()
        self.assertTrue(report["interrupted"])
        self.assertEqual(result.original.digest, source.digest)
        self.assertFalse(report["globally_minimal"])
        self.assertIn("no claim of global minimality", report["claim"])

    async def test_a_report_never_claims_global_minimality(self):
        source = generate(20260911, 3).program
        checker = Recorder(keeps("value"))
        result = await shrink.reduce(
            checker, source, budget=shrink.Budget(candidates=4)
        )
        report = result.report()
        self.assertIs(report["globally_minimal"], False)
        self.assertEqual(report["version"], shrink.VERSION)
        self.assertIn("fixture_sha256", report["original"])
        self.assertIn("fixture_sha256", report["best"])

    async def test_shrinking_requires_a_validated_program(self):
        with self.assertRaises(shrink.ShrinkError):
            await shrink.reduce(Recorder(passing), {"nodes": []})

    async def test_budgets_must_permit_at_least_one_attempt(self):
        with self.assertRaises(shrink.ShrinkError):
            shrink.Budget(candidates=0)
        with self.assertRaises(shrink.ShrinkError):
            shrink.Budget(seconds=0)


# [spec:pgorm:req:generative.shrink/test]
class SequenceTests(unittest.IsolatedAsyncioTestCase):
    async def test_effect_sequences_and_transactions_both_reduce(self):
        source = program_with("begin", limit=60, family="sequence")
        checker = Recorder(keeps("begin"))
        result = await shrink.reduce(checker, source)
        self.assertLessEqual(
            len(result.best.data()["steps"]), len(source.data()["steps"])
        )
        self.assertTrue(
            all(
                shrink._balanced(candidate.data()["steps"])
                for candidate in checker.seen
            )
        )

    async def test_a_reduced_program_still_closes_its_transactions(self):
        source = program_with("begin", limit=60, family="sequence")
        checker = Recorder(keeps("begin"))
        result = await shrink.reduce(checker, source)
        self.assertTrue(shrink._balanced(result.best.data()["steps"]))


if __name__ == "__main__":
    asyncio.run(unittest.main())
