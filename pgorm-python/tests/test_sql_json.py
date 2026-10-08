"""SQL/JSON's functions, constructors and IS JSON against PostgreSQL, through the installed wheel."""

import os
import unittest
from uuid import uuid4

import pgorm as p

DOC = {"tags": ["blue", "red"], "size": 12, "name": "Nora O'Brien", "it's \\ key": "hostile",
       "nested": {"k": None}, "word": "x"}


def one(*items):
    return p.Select(*(item.as_(f"c{index}") for index, item in enumerate(items)))


# [spec:pgorm:req:python.expressions+1/test]
class SqlJsonTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)

    async def asyncTearDown(self):
        await self.pool.close()

    async def values(self, *items):
        row = await self.pool.fetch_one(one(*items))
        return list(row.values())

    async def sqlstate(self, item):
        with self.assertRaises(p.DatabaseError) as caught:
            await self.pool.fetch_one(one(item))
        return caught.exception.sqlstate

    async def test_query_functions_read_bound_and_literal_documents(self):
        for operand in (p.bind, p.literal):
            doc = operand(p.Value.json(DOC))
            with self.subTest(path=operand.__name__):
                self.assertEqual(await self.values(
                    p.json_exists(doc, "$.tags[*] ? (@ == $Tag)", passing={"Tag": "blue"}),
                    p.json_exists(doc, "$.tags[*] ? (@ == $Tag)", passing={"Tag": "green"}),
                    p.json_value(doc, "$.size", returning="integer"),
                    p.json_value(doc, "$.name"),
                    p.json_value(doc, '$."it\'s \\\\ key"'),
                    p.json_query(doc, "$.tags"),
                    p.json_query(doc, "$.nested"),
                ), [True, False, 12, "Nora O'Brien", "hostile", ["blue", "red"], {"k": None}])

    async def test_passing_names_are_case_exact_identifiers(self):
        doc = p.bind(p.Value.json(DOC))
        self.assertEqual(await self.values(
            p.json_value(doc, "$.size ? (@ > $Min && @ < $max)", returning="integer",
                         passing={"Min": 10, "max": p.literal(20)}),
        ), [12])
        # The variable is named $Tag; an unquoted name would fold to tag and miss it.
        self.assertEqual(await self.sqlstate(
            p.json_exists(doc, "$.tags[*] ? (@ == $tag)", passing={"Tag": "blue"},
                          on_error=p.JsonExistsBehavior.Error)), "42704")

    async def test_exists_behaviours_answer_a_failing_path(self):
        doc = p.bind(p.Value.json(DOC))
        strict = "strict $.missing"
        self.assertEqual(await self.values(
            p.json_exists(doc, strict),
            p.json_exists(doc, strict, on_error=p.JsonExistsBehavior.True_),
            p.json_exists(doc, strict, on_error=p.JsonExistsBehavior.False_),
            p.json_exists(doc, strict, on_error=p.JsonExistsBehavior.Unknown),
        ), [False, True, False, None])
        self.assertEqual(await self.sqlstate(
            p.json_exists(doc, strict, on_error=p.JsonExistsBehavior.Error)), "2203A")

    async def test_value_behaviours_and_hostile_defaults(self):
        doc = p.bind(p.Value.json(DOC))
        self.assertEqual(await self.values(
            p.json_value(doc, "$.missing", returning="integer", on_empty=p.JsonDefault(-1)),
            p.json_value(doc, "$.name", returning="integer", on_error=p.JsonDefault(-2)),
            p.json_value(doc, "$.missing", on_empty=p.JsonDefault("it's'); DROP TABLE x; --")),
            p.json_value(doc, "$.tags", on_error=p.JsonValueBehavior.Null),
        ), [-1, -2, "it's'); DROP TABLE x; --", None])
        self.assertEqual(await self.sqlstate(
            p.json_value(doc, "$.missing", on_empty=p.JsonValueBehavior.Error)), "22035")
        self.assertEqual(await self.sqlstate(
            p.json_value(doc, "$.tags", on_error=p.JsonValueBehavior.Error)), "2203F")
        with self.assertRaises(p.ConstructionError):
            p.json_value(doc, "$.size", returning="jsonb")

    async def test_query_shaping_and_behaviours(self):
        doc = p.bind(p.Value.json(DOC))
        self.assertEqual(await self.values(
            p.json_query(doc, "$.tags[*]", shaping="with_wrapper"),
            p.json_query(doc, "$.size", shaping="with_wrapper"),
            p.json_query(doc, "$.size", shaping="with_conditional_wrapper"),
            p.json_query(doc, "$.word", shaping="omit_quotes", returning="text"),
            p.json_query(doc, "$.word", returning="text"),
            p.json_query(doc, "$.missing", on_empty=p.JsonQueryBehavior.EmptyArray),
            p.json_query(doc, "$.tags[*]", on_error=p.JsonQueryBehavior.EmptyObject),
            p.json_query(doc, "$.missing", on_empty=p.JsonDefault(p.Value.json({"d": 1}))),
        ), [["blue", "red"], [12], 12, "x", '"x"', [], {}, {"d": 1}])
        self.assertEqual(await self.sqlstate(
            p.json_query(doc, "$.tags[*]", on_error=p.JsonQueryBehavior.Error)), "22034")

    async def test_constructors_type_their_values(self):
        self.assertEqual(await self.values(
            p.json_object({"id": 7, "name": "O'Brien", "doc": p.format_json(p.bind('{"x":1}'))},
                          returning="jsonb"),
            p.json_object([("a", p.Value.null("text")), ("b", 1)], absent_on_null=True,
                          returning="jsonb"),
            p.json_array(1, "a", p.Value.null("text"), True, returning="jsonb"),
            p.json_array(1, p.Value.null("text"), null_on_null=True, returning="jsonb"),
            p.json_scalar(5),
            p.json_scalar("5"),
            p.json_parse('{"a": [1, 2]}'),
            p.json_serialize(p.literal(p.Value.json({"a": 1, "b": 2}))),
        ), [{"id": 7, "name": "O'Brien", "doc": {"x": 1}}, {"b": 1}, [1, "a", True], [1, None],
            5, "5", {"a": [1, 2]}, '{"a":1,"b":2}'])
        self.assertEqual(await self.sqlstate(
            p.json_object([("a", 1), ("a", 2)], unique_keys=True)), "22030")
        self.assertEqual(await self.sqlstate(p.json_parse('{"a":1,"a":2}', unique_keys=True)), "22030")
        self.assertEqual(await self.values(p.json_parse('{"a":1,"a":2}')), [{"a": 2}])

    async def test_aggregates_order_filter_and_array_query(self):
        name = "python_json_" + uuid4().hex
        table = p.Table(name)
        async with self.pool.connection() as connection:
            await connection.execute(p.RawSQL(f'CREATE TEMP TABLE "{name}" (k text, n integer)'))
            await connection.execute(
                p.Insert(table).columns("k", "n").values("a", 1).values("b", 2).values("c", p.Value.null("i32")))
            query = p.Select(
                p.json_arrayagg(p.col("n"), order_by=[p.col("k").desc()], returning="jsonb").as_("desc"),
                p.json_arrayagg(p.col("n"), order_by=[p.col("k").asc()], null_on_null=True,
                                returning="jsonb").as_("nulls"),
                p.json_objectagg(p.col("k"), p.col("n"), filter=p.col("n") > 1,
                                 returning="jsonb").as_("filtered"),
                p.json_objectagg(p.col("k"), p.col("n"), absent_on_null=True,
                                 returning="jsonb").as_("absent"),
                p.json_array_query(p.Select(p.col("n")).from_(table).where_(p.col("n") < 2),
                                   returning="jsonb").as_("sub"),
            ).from_(table)
            row = await connection.fetch_one(query)
            self.assertEqual(dict(row), {"desc": [2, 1], "nulls": [1, 2, None], "filtered": {"b": 2},
                                         "absent": {"a": 1, "b": 2}, "sub": [1]})

    async def test_is_json_tests_kind_and_unique_keys(self):
        cases = [
            ('{"a":1}', p.JsonKind.Object, False, True),
            ('{"a":1}', p.JsonKind.Array, False, False),
            ("[1]", p.JsonKind.Array, False, True),
            ("1", p.JsonKind.Scalar, False, True),
            ('{"a":1,"a":2}', p.JsonKind.Object, False, True),
            ('{"a":1,"a":2}', p.JsonKind.Object, True, False),
            ("{not json", p.JsonKind.Value, False, False),
        ]
        for text, kind, unique, expected in cases:
            with self.subTest(text=text, kind=kind, unique=unique):
                self.assertEqual(await self.values(
                    p.is_json(text, kind, unique_keys=unique),
                    p.is_not_json(text, kind, unique_keys=unique),
                ), [expected, not expected])

    async def test_jsonb_serializes_as_its_document(self):
        self.assertEqual(await self.values(
            p.json_serialize(p.bind(p.Value.json({"a": 1, "b": 2}))),
            p.json_serialize(p.bind(p.Value.json([1])), returning="bytea"),
        ), ['{"a": 1, "b": 2}', b"[1]"])
