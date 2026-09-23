import unittest

from pgorm_campaign.comparison import InvalidOracle
from pgorm_campaign.reference import Resolution

from oracle_pipeline_cases import ordered


class PipelineReferenceTests(unittest.TestCase):
    # [spec:pgorm:req:generative.oracles/test]
    def test_hidden_sort_key_survives_projection_and_pagination(self):
        program = ordered("hidden").data()
        query = Resolution(program).get(program["steps"][0]["inputs"]["query"])
        statement, _ = query.terminal().command()
        self.assertEqual([name for _, name in query.columns], ["name"])
        self.assertEqual(query.ordering[0].direction, "DESC")
        self.assertIsNone(query.ordering[0].visible)
        self.assertIn('ORDER BY "q"."o0" DESC OFFSET 1 LIMIT 2', statement)
        self.assertTrue(statement.endswith('ORDER BY "q"."o0" DESC'))
        with self.assertRaises(InvalidOracle):
            query.wrap(query.projected(), distinct=True)

    def test_distinct_requires_a_later_explicit_sort(self):
        program = ordered("distinct").data()
        resolution = Resolution(program)
        distinct = next(
            node for node in program["nodes"] if node["op"] == "pipeline.distinct"
        )
        self.assertEqual(resolution.get(distinct["id"]).ordering, ())
        query = resolution.get(program["steps"][0]["inputs"]["query"])
        self.assertEqual(query.ordering[0].visible, 0)
        statement, _ = query.terminal().command()
        self.assertIn("SELECT DISTINCT", statement)
        self.assertTrue(statement.endswith('ORDER BY "q"."o0" DESC'))


class IdentifierScreenTests(unittest.TestCase):
    # [spec:pgorm:req:generative.oracles/test]
    def test_quoted_pipeline_alias_is_refused_by_name(self):
        from pgorm_campaign.grammar import generate
        from pgorm_campaign.reference_sql import Rejection

        for index in range(40):
            generated = generate(5, index, mode="invalid")
            if generated.recipe()["rejection_case"] == "unquotable-identifier":
                break
        program = generated.program.data()
        step = program["steps"][0]
        declared = next(
            item["error"]
            for item in program["observations"]
            if item["step"] == step["id"]
        )
        with self.assertRaises(Rejection) as raised:
            Resolution(program).get(step["inputs"]["query"])
        self.assertEqual(
            raised.exception.observation,
            {
                "kind": "error",
                "class": declared["class"],
                "cause": declared["cause"],
                "sqlstate": None,
            },
        )

    def test_screen_reads_tables_and_identifier_fields(self):
        from pgorm_campaign.reference_pipeline import identifiers
        from pgorm_campaign.reference_sql import Table

        self.assertEqual(
            identifiers(
                "pipeline.source",
                {"source": Table('odd" 雪', "fixture")},
                {"alias": "a"},
            ),
            ["a", "fixture", 'odd" 雪'],
        )
        self.assertEqual(
            identifiers("pipeline.sources", {}, {"qualifiers": ["x", "y"]}),
            ["x", "y"],
        )
