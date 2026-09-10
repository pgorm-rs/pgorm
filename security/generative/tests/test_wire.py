import json
import unittest

from pgorm_campaign import wire


# [spec:pgorm:req:generative.format/test]
class WireTests(unittest.TestCase):
    def test_exact_scalar_tags_and_bits_roundtrip(self):
        cases = [
            ("i64", "-9223372036854775808"),
            ("u64", "18446744073709551615"),
            ("f32", "80000000"),
            ("f32", "7fc00001"),
            ("f64", "7ff0000000000000"),
            ("decimal", "-0.00100"),
            ("bytes", [0, 255, 39]),
            ("json", None),
            ("datetime_fixed", "2024-01-02 03:04:05.123456+02:00"),
            ("date", "2024-01-02"),
            ("time", "03:04:05.123456"),
            ("time", "03:04:05.123"),
            ("datetime_utc", "2024-01-02 03:04:05 UTC"),
            ("datetime_fixed", "2024-01-02 03:04:05.123 +02:00"),
            ({"kind": "enum", "schema": "fixture", "name": 'State" 雪'}, "O'Brien 雪"),
        ]
        for kind, data in cases:
            value = wire.scalar(kind, data)
            self.assertEqual(value, wire.validate(json.loads(json.dumps(value))))

    def test_null_and_array_identities_are_distinct(self):
        sql_null = wire.scalar("json", None, sql_null=True)
        json_null = wire.scalar("json", None)
        self.assertNotEqual(sql_null, json_null)
        array = wire.scalar(
            {"kind": "array", "element": {"kind": "json"}}, [sql_null, json_null]
        )
        self.assertEqual(array["data"], [sql_null, json_null])
        self.assertNotEqual(
            wire.scalar("f64", "8000000000000000"),
            wire.scalar("f64", "0000000000000000"),
        )

    def test_precision_and_type_loss_are_rejected(self):
        for kind, data in (
            ("i8", "128"),
            ("u64", "-1"),
            ("i32", 7),
            ("i32", "01"),
            ("f32", 1.5),
            ("f64", "7ff000000000000"),
            ("bool", 1),
            ("bytes", [256]),
            ("bytes", [True]),
            ("decimal", "1e-29"),
            ("decimal", "79228162514264337593543950336"),
            ("time", "03:04:05.123456789"),
            ("datetime", "2024-01-02 03:04:05+00:00"),
            ("json", 2**64),
            ("json", float("nan")),
            ({"kind": "array", "element": {"kind": "i32"}}, [wire.scalar("i64", "1")]),
        ):
            with (
                self.subTest(kind=kind, data=data),
                self.assertRaises(wire.FormatError),
            ):
                wire.scalar(kind, data)


if __name__ == "__main__":
    unittest.main()
