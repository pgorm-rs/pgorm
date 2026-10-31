"""CASE against PostgreSQL, through the installed wheel: each searched and
simple CASE answers as the CASE written by hand as SQL text answers."""

import os
import unittest
from uuid import uuid4

import pgorm as p


def quoted(name):
    return '"' + name.replace('"', '""') + '"'


# [spec:pgorm:req:bindings.case/test]    searched and simple CASE, with and without an ELSE,
# go where an expression goes (a projection, WHERE, ORDER BY and an UPDATE's value), each
# value and result bound unless an expression, and answer as the CASE written by hand
class CaseTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
        self.context = self.pool.connection()
        self.connection = await self.context.__aenter__()
        self.name = f"cases {uuid4().hex[:8]}"
        await self.connection.execute(p.RawSQL(
            f"CREATE TEMP TABLE {quoted(self.name)} "
            "(id int PRIMARY KEY, score int, mood text, kind text NOT NULL)"))
        await self.connection.execute(p.RawSQL(
            f"INSERT INTO {quoted(self.name)} VALUES "
            "(1, 20, 'calm', 'a'), (2, 5, NULL, 'b'), (3, NULL, 'glad', 'c')"))
        self.table = p.Table(self.name)

    async def asyncTearDown(self):
        await self.context.__aexit__(None, None, None)
        await self.pool.close()

    def c(self, name):
        return self.table.col(name)

    async def built(self, expression):
        query = (p.Select(self.c("id"), expression.as_("v")).from_(self.table)
                 .order_by(self.c("id").asc()))
        return [(row["id"], row["v"]) for row in await self.connection.query(query)]

    async def written(self, expression):
        rows = await self.connection.query(p.RawSQL(
            f"SELECT id, {expression} AS v FROM {quoted(self.name)} ORDER BY id"))
        return [(row["id"], row["v"]) for row in rows]

    async def test_each_case_answers_as_postgres_does(self):
        score, mood, kind = self.c("score"), self.c("mood"), self.c("kind")
        cases = [
            (p.case_when(score > 10, "high").when(score.is_null(), "none").else_("low"),
             "CASE WHEN score > 10 THEN 'high' WHEN score IS NULL THEN 'none' ELSE 'low' END",
             [(1, "high"), (2, "low"), (3, "none")]),
            (p.case_when(p.Condition.any(score < 10, mood == "glad"), score * 2),
             "CASE WHEN score < 10 OR mood = 'glad' THEN score * 2 END",
             [(1, None), (2, 10), (3, None)]),
            (p.case_of(mood).when("calm", "yes").else_("no"),
             "CASE mood WHEN 'calm' THEN 'yes' ELSE 'no' END",
             [(1, "yes"), (2, "no"), (3, "no")]),
            (p.case_of(kind).when("a", p.literal(1)).when(p.literal("b"), self.c("id") + 10),
             "CASE kind WHEN 'a' THEN 1 WHEN 'b' THEN id + 10 END",
             [(1, 1), (2, 12), (3, None)]),
            (p.case_of(self.c("id") % 2).when(0, "even").else_(kind).concat("!"),
             "(CASE id % 2 WHEN 0 THEN 'even' ELSE kind END) || '!'",
             [(1, "a!"), (2, "even!"), (3, "c!")]),
        ]
        for expression, written, expected in cases:
            with self.subTest(written=written):
                rows = await self.built(expression)
                self.assertEqual(rows, await self.written(written))
                self.assertEqual(rows, expected)

    async def test_a_case_filters_orders_and_sets(self):
        score, kind = self.c("score"), self.c("kind")
        first = p.case_of(kind).when("c", p.literal(0)).else_(p.literal(1))
        query = (p.Select(self.c("id")).from_(self.table)
                 .where(p.case_when(score.is_null(), True).else_(score > 10))
                 .order_by(first.asc(), self.c("id").desc()))
        ids = [row["id"] for row in await self.connection.query(query)]
        rows = await self.connection.query(p.RawSQL(
            f"SELECT id FROM {quoted(self.name)} "
            "WHERE CASE WHEN score IS NULL THEN true ELSE score > 10 END "
            "ORDER BY CASE kind WHEN 'c' THEN 0 ELSE 1 END, id DESC"))
        self.assertEqual(ids, [row["id"] for row in rows])
        self.assertEqual(ids, [3, 1])
        update = (p.Update(self.table)
                  .set("score", p.case_when(score.is_null(), 0).else_(score + 1))
                  .all_rows())
        self.assertEqual(await self.connection.execute(update), 3)
        rows = await self.connection.query(p.RawSQL(
            f"SELECT score FROM {quoted(self.name)} ORDER BY id"))
        self.assertEqual([row["score"] for row in rows], [21, 6, 0])

    def test_an_armless_operand_is_no_expression(self):
        operand = p.case_of(self.c("kind"))
        self.assertNotIsInstance(operand, p.Expr)
        self.assertIsInstance(operand.when("a", 1), p.Expr)
        self.assertIsInstance(p.case_when(self.c("id") > 1, 1), p.Expr)
        with self.assertRaises(TypeError):
            p.Select(operand)
        with self.assertRaises(p.ConstructionError):
            self.c("kind") == operand
        with self.assertRaises(p.ConstructionError):
            bool(operand)
        with self.assertRaises(TypeError):
            p.case_when("score > 1", 1)
        with self.assertRaises(p.ConstructionError):
            p.case_when(self.c("id") > 1, None)
        ended = p.case_when(self.c("id") > 1, 1).else_(0)
        self.assertFalse(hasattr(ended, "when"))
        self.assertFalse(hasattr(ended, "else_"))


if __name__ == "__main__":
    unittest.main()
