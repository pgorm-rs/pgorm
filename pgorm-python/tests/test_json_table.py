"""JSON_TABLE as a FROM item against PostgreSQL, through the installed wheel."""

import os
import unittest
from uuid import uuid4

import pgorm as p

C = p.JsonTableColumn
DOCS = [
    (1, {"items": [{"n": 1, "label": "one", "tags": ["a"], "flag": True, "parts": ["x", "y"]},
                   {"n": 2, "tags": [], "parts": []}]}),
    (2, {"items": [{"n": "three", "label": "three"}]}),
]


def quoted(name):
    return '"' + name.replace('"', '""') + '"'


# [spec:pgorm:req:python.statements+3/test]
class JsonTableTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
        self.context = self.pool.connection()
        self.connection = await self.context.__aenter__()
        self.name = f'Json "docs" {uuid4().hex[:8]}'
        await self.connection.execute(p.RawSQL(
            f"CREATE TEMP TABLE {quoted(self.name)} (id integer PRIMARY KEY, doc jsonb NOT NULL)"))
        insert = p.Insert(p.Table(self.name)).columns("id", "doc")
        for key, doc in DOCS:
            insert = insert.values(key, p.Value.json(doc))
        await self.connection.execute(insert)
        self.docs = p.Table(self.name).as_("d")

    async def asyncTearDown(self):
        await self.context.__aexit__(None, None, None)
        await self.pool.close()

    async def rows(self, query):
        return [tuple(row.values()) for row in await self.connection.fetch_all(query)]

    async def sqlstate(self, query):
        with self.assertRaises(p.DatabaseError) as caught:
            await self.connection.fetch_all(query)
        return caught.exception.sqlstate

    def over(self, column, *, id=2, path="$.items[*]", **options):
        d = self.docs
        jt = p.json_table(d.col("doc"), path, column, alias="jt", **options)
        return p.Select(jt.col("v")).from_(d).from_(jt).where_(d.col("id") == id)

    async def test_column_kinds_read_document_rows(self):
        d = self.docs
        jt = p.json_table(
            d.col("doc"), "$.items[*]", C.ordinality("i"),
            C.value("n", "integer"), C.value("label", "text"),
            C.query("tags", "jsonb", on_empty=p.JsonQueryBehavior.EmptyArray),
            C.exists("flagged", "boolean", path="$.flag"),
            C.nested("$.parts[*]", C.value("part", "text", path="$")),
            alias="jt")
        query = (p.Select(d.col("id"), jt.col("i"), jt.col("n"), jt.col("label"), jt.col("tags"),
                          jt.col("flagged"), jt.col("part"))
                 .from_(d).from_(jt)
                 .order_by(d.col("id").asc(), jt.col("i").asc(), jt.col("part").asc()))
        # A value that does not convert is NULL by default, and an item whose
        # nested path finds nothing keeps one row, as an outer join would.
        self.assertEqual(await self.rows(query), [
            (1, 1, 1, "one", ["a"], True, "x"),
            (1, 1, 1, "one", ["a"], True, "y"),
            (1, 2, 2, None, [], False, None),
            (2, 1, None, "three", [], False, None),
        ])

    async def test_column_behaviours_decide_what_a_row_holds(self):
        self.assertEqual(await self.rows(self.over(
            C.value("v", "integer", path="$.n", on_error=p.JsonDefault(-1)))), [(-1,)])
        self.assertEqual(await self.sqlstate(self.over(
            C.value("v", "integer", path="$.n", on_error=p.JsonValueBehavior.Error))), "22P02")
        self.assertEqual(await self.rows(self.over(
            C.value("v", "text", path="$.missing", on_empty=p.JsonDefault("it's'); DROP TABLE x; --")))),
            [("it's'); DROP TABLE x; --",)])
        self.assertEqual(await self.rows(self.over(
            C.exists("v", "boolean", path="strict $.missing", on_error=p.JsonExistsBehavior.Unknown))),
            [(None,)])
        self.assertEqual(await self.rows(self.over(
            C.query("v", "jsonb", path="$.label", shaping="with_wrapper"))), [(["three"],)])
        self.assertEqual(await self.rows(self.over(
            C.query("v", "text", path="$.label", shaping="omit_quotes"))), [("three",)])

    async def test_table_behaviour_answers_failing_root_path(self):
        strict = "strict $.missing[*]"
        self.assertEqual(await self.rows(self.over(C.ordinality("v"), path=strict)), [])
        self.assertEqual(await self.rows(self.over(
            C.ordinality("v"), path=strict, on_error=p.JsonTableBehavior.Empty)), [])
        self.assertEqual(await self.sqlstate(self.over(
            C.ordinality("v"), path=strict, on_error=p.JsonTableBehavior.Error)), "2203A")

    async def test_hostile_path_stays_literal_data(self):
        await self.connection.execute(p.Insert(p.Table(self.name)).columns("id", "doc")
                                      .values(3, p.Value.json({"it's \\ here": {"v": "found"}})))
        self.assertEqual(await self.rows(self.over(
            C.value("v", "text", path="$.v"), id=3, path='$."it\'s \\\\ here"')), [("found",)])
        compiled = self.over(C.value("v", "text", path="$.v"), id=3, path="$.it's").inspect()
        self.assertIn("JSON_TABLE(\"d\".\"doc\", E'$.it\\'s' COLUMNS", compiled.sql)
        self.assertEqual([value.value for value in compiled.params], [3])

    async def test_passing_and_names_are_case_exact_identifiers(self):
        d = self.docs
        jt = p.json_table(
            d.col("doc"), "$.items[*] ? (@.n >= $Min)", C.value("Label", "text", path="$.label"),
            C.nested("$.parts[*]", C.value('a "part"', "text", path="$"), path_name="Parts"),
            alias='J "t"', passing={"Min": 1}, path_name="Root")
        query = (p.Select(jt.col("Label"), jt.col('a "part"')).from_(d).from_(jt)
                 .order_by(jt.col('a "part"').asc()))
        self.assertEqual(await self.rows(query), [("one", "x"), ("one", "y"), (None, None)])

    async def test_left_join_keeps_empty_document_row(self):
        d = self.docs
        flagged = p.json_table(d.col("doc"), "$.items[*] ? (@.flag == true)", C.value("label", "text"),
                               alias="jt")
        query = (p.Select(d.col("id"), flagged.col("label")).from_(d)
                 .join(flagged, p.literal(True), kind=p.Join.Left).order_by(d.col("id").asc()))
        self.assertEqual(await self.rows(query), [(1, "one"), (2, None)])

    async def test_server_refuses_repeated_name_and_bad_format(self):
        d = self.docs
        twice = p.json_table(d.col("doc"), "$", C.ordinality("x"), C.value("x", "text"), alias="jt")
        self.assertEqual(await self.sqlstate(p.Select().from_(d).from_(twice)), "42712")
        formatted = p.json_table(d.col("doc"), "$.items[*]", C.query("n", "integer"), alias="jt")
        self.assertEqual(await self.sqlstate(p.Select().from_(d).from_(formatted)), "0A000")

    def test_table_needs_column_and_alias(self):
        with self.assertRaises(TypeError):
            p.json_table(p.col("doc"), "$", alias="t")
        with self.assertRaises(TypeError):
            p.json_table(p.col("doc"), "$", C.ordinality("i"))
        with self.assertRaises(TypeError):
            C.nested("$")
        with self.assertRaises(p.ConstructionError):
            p.Select().from_("docs")
        self.assertEqual(p.json_table(p.col("doc"), "$", C.ordinality("i"), alias="t").alias, "t")
