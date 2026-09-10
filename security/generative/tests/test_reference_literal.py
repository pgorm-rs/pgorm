import unittest

from pgorm_campaign import wire
from pgorm_campaign.reference_literal import literal


class LiteralReferenceTests(unittest.TestCase):
    def test_numeric_literal_inference_differs_from_wire_storage(self):
        cases = (
            (wire.scalar("i64", "1"), "integer", "1"),
            (wire.scalar("i64", "2147483648"), "bigint", "2147483648"),
            (
                wire.scalar("u64", "18446744073709551615"),
                "numeric",
                "18446744073709551615",
            ),
            (wire.scalar("f32", "3dcccccd"), "numeric", "0.1"),
            (wire.scalar("f64", "8000000000000000"), "integer", "-0"),
            (wire.scalar("decimal", "1.000"), "numeric", "1.000"),
        )
        for value, kind, text in cases:
            with self.subTest(value=value):
                self.assertEqual(literal(value).command(), ("%s::" + kind, [text]))
        self.assertEqual(
            literal(wire.scalar("f64", "3ff0000000000000"), pipeline=True).command(),
            ("%s::numeric", ["1.0"]),
        )

    def test_unknown_literals_remain_bound_for_contextual_inference(self):
        text = "O'Brien; -- %_\\ 雪"
        self.assertEqual(literal(wire.scalar("text", text)).command(), ("%s", [text]))
        self.assertEqual(
            literal(wire.scalar("uuid", None, sql_null=True)).command(), ("%s", [None])
        )
        self.assertEqual(
            literal(wire.scalar("time", "03:04:05.123456")).command(),
            ("%s", ["03:04:05"]),
        )
