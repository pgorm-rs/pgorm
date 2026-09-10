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
