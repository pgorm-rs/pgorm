import copy
import json
import unittest

from pgorm_campaign import baseline as b


# [spec:pgorm:req:generative.fixtures/test]
class BaselineTests(unittest.TestCase):
    def test_portable_definition_preserves_fixture_shapes(self):
        value = b.default()
        self.assertEqual(b.digest(value), b.digest(json.loads(json.dumps(value))))
        accounts = value["tables"][0]
        self.assertEqual({row[1] for row in accounts["rows"]}, {1, 2})
        self.assertEqual(accounts["rows"][0][5], accounts["rows"][1][5])
        self.assertIsNone(accounts["rows"][0][3])
        names = [(table["schema"], table["name"]) for table in value["tables"]]
        self.assertIn(("fixture", "accounts"), names)
        self.assertIn(("other", "accounts"), names)
        self.assertIn(("fixture", "sentinels"), names)

    def test_fixture_names_and_values_remain_data(self):
        sql = b.render(b.default())
        self.assertIn('"fixture"."odd"" 雪"', sql)
        self.assertIn("'O''Brien 雪'", sql)
        self.assertIn('"fixture"."State"" 雪"', sql)
        self.assertEqual(b.text("x'; COMMIT; --"), "'x''; COMMIT; --'")
        self.assertEqual(b.identifier('x"; --'), '"x""; --"')
        self.assertIn("SET standard_conforming_strings = on;", sql)

    def test_restore_retains_enum_and_table_identity(self):
        sql = b.restore(b.default())
        self.assertNotIn("DROP", sql)
        self.assertNotIn("CREATE", sql)
        self.assertIn("TRUNCATE", sql)
        self.assertEqual(sql.count("INSERT INTO"), 5)

    def test_invalid_definitions_fail_without_execution(self):
        for change in (
            lambda v: v.update(version=True),
            lambda v: v.update(tables=[]),
            lambda v: v["tables"][0].update(schema="pg_catalog"),
            lambda v: v["tables"][0]["columns"][0].update(
                kind="integer; DROP DATABASE postgres"
            ),
            lambda v: v["tables"][0]["columns"][0].update(name="\x00"),
            lambda v: v["tables"][0]["rows"][0].__setitem__(0, None),
            lambda v: v["tables"][0]["rows"][0].append("extra"),
            lambda v: v["tables"].append(copy.deepcopy(v["tables"][0])),
        ):
            value = b.default()
            change(value)
            with self.subTest(value=value), self.assertRaises(ValueError):
                b.render(value)


if __name__ == "__main__":
    unittest.main()
