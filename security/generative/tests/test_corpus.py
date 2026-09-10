from hashlib import sha256
import json
import sys
import unittest

from pgorm_campaign import wire
from pgorm_campaign.author import Author
from pgorm_campaign.corpus import Input, encoded
from pgorm_campaign.corpus_builtin import builtin, ENUM, FLOATS, STRINGS
from pgorm_campaign.corpus_random import sample


# [spec:pgorm:req:generative.corpus/test]
class CorpusTests(unittest.TestCase):
    def test_required_boundaries_preserve_tags_and_bytes(self):
        cases = builtin()
        self.assertEqual(len(cases), 308)
        self.assertEqual(len({case.data()["id"] for case in cases}), len(cases))
        self.assertTrue(set(STRINGS) <= {case.data()["family"] for case in cases})
        values = [wire.validate(case.value()) for case in cases]
        for kind, (bits, signed) in wire.INTEGER_BITS.items():
            numbers = {
                int(value["data"])
                for value in values
                if value["type"]["kind"] == kind and not value["sql_null"]
            }
            self.assertEqual(min(numbers), -(2 ** (bits - 1)) if signed else 0)
            self.assertEqual(max(numbers), 2 ** (bits - int(signed)) - 1)
        for kind, patterns in FLOATS.items():
            self.assertEqual(
                {
                    value["data"]
                    for value in values
                    if value["type"]["kind"] == kind and not value["sql_null"]
                },
                set(patterns),
            )
        for kind in wire.SCALARS:
            self.assertIn(wire.scalar(kind, None, sql_null=True), values)
            self.assertIn(
                wire.scalar(
                    {"kind": "array", "element": {"kind": kind}}, [], sql_null=False
                ),
                values,
            )
        self.assertIn(wire.scalar(ENUM, "O'Brien 雪"), values)
        self.assertIn(wire.scalar("json", None), values)
        self.assertIn(wire.scalar("decimal", "-0.00"), values)

    def test_identifier_traits_do_not_invent_rejections(self):
        names = {
            case.value()["data"]: case.identifier_traits()
            for case in builtin()
            if "identifier" in case.data()["roles"]
        }
        self.assertTrue(names[""]["empty"])
        self.assertTrue(names["a\0b"]["contains_nul"])
        for text in ("a" * 63, "雪" * 21, "雪" * 21 + "a", "雪" * 21 + "aa"):
            self.assertEqual(names[text]["utf8_bytes"], len(text.encode("utf-8")))
            self.assertNotIn("expected_error", names[text])

    def test_payload_can_only_author_data_nodes(self):
        attack = Input.create(
            "test", "hostile", wire.scalar("text", "');SELECT 1;--"), {}
        )
        for role, expected in (("value", ["value"]), ("identifier", ["value", "name"])):
            author = Author()
            attack.node(author, role=role)
            self.assertEqual([node["op"] for node in author.nodes], expected)
            self.assertEqual(author.nodes[0]["data"]["value"], attack.value())
        for role in ("raw", "expr.binary", "condition", "sql"):
            author = Author()
            with self.assertRaises(ValueError):
                attack.node(author, role=role)
            self.assertEqual(author.nodes, [])
        with self.assertRaises(ValueError):
            Input.create(
                "null", "null", wire.scalar("text", None, sql_null=True), {}
            ).node(Author(), role="identifier")

    def test_case_loading_is_immutable_and_versioned(self):
        case = builtin()[0]
        copy = case.data()
        copy["value"]["data"] = "changed"
        self.assertNotEqual(copy, case.data())
        self.assertEqual(Input.from_data(json.loads(encoded(case.data()))), case)
        for key, value in (("version", 2), ("version", True), ("roles", ["raw"])):
            copy = case.data()
            copy[key] = value
            with self.assertRaises(ValueError):
                Input.from_data(copy)

    def test_random_values_reproduce_across_partition_orders(self):
        values = [sample(123, index) for index in range(1000)]
        self.assertEqual(
            sha256(b"".join(encoded(item.data()) for item in values)).hexdigest(),
            "fe41fe1bdb11f15802f23e59d39f2d906cfa8f748d9cdcd00e2375f546295ac4",
        )
        self.assertEqual(
            list(reversed(values)),
            [sample(123, index) for index in reversed(range(1000))],
        )
        self.assertGreater(len({encoded(item.value()) for item in values}), 900)
        self.assertNotEqual(
            [sample(123, index).value() for index in range(32)],
            [sample(124, index).value() for index in range(32)],
        )
        self.assertNotIn("pgorm", sys.modules)
        self.assertFalse(
            any(name == "sqlmap" or name.startswith("sqlmap.") for name in sys.modules)
        )
        for seed, index in ((True, 0), (-1, 0), (0, 2**64)):
            with self.assertRaises(ValueError):
                sample(seed, index)
