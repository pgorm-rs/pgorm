"""Coverage is discharged by observed construction and dispatch, not by labels."""

import unittest

from pgorm_campaign import campaign_coverage, campaign_plan, grammar, profiles


def executed(program, *, registration=None):
    """A synthetic report in which every instruction was actually dispatched."""
    data = program.data()
    trace = [
        {
            "id": node["id"],
            "operation": node["op"],
            "status": "constructed",
            "native_paths": ["pgorm_query::Value"],
            "observation": {},
        }
        for node in data["nodes"]
    ]
    if registration is not None:
        trace.append(
            {
                "id": "registered",
                "status": "constructed",
                "native_paths": ["pgorm::EntityTrait"],
                "observation": {"registration": registration},
            }
        )
    return {
        "status": "pass",
        "subject": {
            "trace": trace,
            "steps": [
                {"id": step["id"], "status": "observed", "native_paths": ["pgorm"]}
                for step in data["steps"]
            ],
            "cleanup_errors": [],
            "builds": 0,
        },
        "comparisons": [],
    }


def unexecuted(program):
    return {"status": "incomplete", "subject": {"trace": [], "steps": []}}


class ObservedCoverageTest(unittest.TestCase):
    # [spec:pgorm:req:generative.verdict/test]
    def test_dispatched_operations_and_effects_are_observed(self):
        program = grammar.generate(20260913, 0, family="select").program
        tokens = campaign_coverage.observed(program.data(), executed(program))
        data = program.data()
        for node in data["nodes"]:
            self.assertIn("operation." + node["op"], tokens)
        for step in data["steps"]:
            self.assertIn("effect." + step["op"], tokens)

    # [spec:pgorm:req:generative.verdict/test]
    def test_a_program_that_never_ran_observes_nothing(self):
        program = grammar.generate(20260913, 0, family="select").program
        self.assertEqual(
            campaign_coverage.observed(program.data(), unexecuted(program)), set()
        )

    # [spec:pgorm:req:generative.verdict/test]
    def test_value_contexts_come_from_the_instruction(self):
        program = grammar.generate(20260913, 8, family="types").program
        tokens = campaign_coverage.observed(program.data(), executed(program))
        contexts = {
            token.split(".")[-1] for token in tokens if token.startswith("value.")
        }
        self.assertTrue(contexts)
        self.assertTrue(contexts <= {"literal", "bound", "typed-null", "result-decode"})

    # [spec:pgorm:req:generative.verdict/test]
    def test_registration_evidence_is_attributed(self):
        program = grammar.generate(20260913, 3, family="entity").program
        tokens = campaign_coverage.observed(
            program.data(), executed(program, registration="campaign.Account")
        )
        self.assertIn("registered_entities.campaign.Account", tokens)


class DeclaredCoverageTest(unittest.TestCase):
    def setUp(self):
        self.profile = profiles.select("smoke")
        self.plan = campaign_plan.schedule(self.profile)

    # [spec:pgorm:req:generative.profiles/test]
    def test_smoke_requires_each_scheduled_family(self):
        required = campaign_coverage.required(self.profile, self.plan)
        self.assertIn("family.select", required)
        self.assertIn("family.rejection", required)
        self.assertIn("class.control", required)
        self.assertNotIn("operation.value", required)

    # [spec:pgorm:req:generative.profiles/test]
    def test_full_requires_the_whole_matrix(self):
        profile = profiles.select("full")
        required = campaign_coverage.required(profile, campaign_plan.schedule(profile))
        self.assertIn("operation.value", required)
        self.assertIn("value.bool.literal", required)
        self.assertGreater(len(required), 400)

    # [spec:pgorm:req:generative.verdict/test]
    def test_missing_family_leaves_coverage_unsatisfied(self):
        required = campaign_coverage.required(self.profile, self.plan)
        result = campaign_coverage.assess(
            self.profile, self.plan, required - {"family.pipeline"}
        )
        self.assertFalse(result["satisfied"])
        self.assertEqual(result["declared_missing"], ["family.pipeline"])

    # [spec:pgorm:req:generative.artifacts/test]
    def test_smoke_records_the_full_matrix_gap(self):
        required = campaign_coverage.required(self.profile, self.plan)
        result = campaign_coverage.assess(self.profile, self.plan, required)
        self.assertTrue(result["satisfied"])
        self.assertFalse(result["full_matrix"]["claimed"])
        self.assertGreater(result["full_matrix"]["outstanding"], 0)
        self.assertIn("does not establish", result["claim"])


if __name__ == "__main__":
    unittest.main()
