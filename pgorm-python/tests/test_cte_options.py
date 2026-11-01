"""A common table expression's column list and materialization, and WITH
RECURSIVE with SEARCH and CYCLE, against PostgreSQL through the installed
wheel."""

import os
import unittest
from uuid import uuid4

import pgorm as p


def quoted(name):
    return '"' + name.replace('"', '""') + '"'


def int4(value):
    return p.bind(value).cast(p.TypeName("int4"))


# [spec:pgorm:req:bindings.cte-options/test]
class CteOptionTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
        self.context = self.pool.connection()
        self.connection = await self.context.__aenter__()
        suffix = uuid4().hex[:8]
        self.events_name, self.edges_name = f'Ctes "events" {suffix}', f"ctes edges {suffix}"
        await self.connection.execute(p.RawSQL(
            f"CREATE TEMP TABLE {quoted(self.events_name)} (account_id int NOT NULL, kind text NOT NULL)"))
        await self.connection.execute(p.RawSQL(f"CREATE TEMP TABLE {quoted(self.edges_name)} (a int, b int)"))
        self.events, self.edges = p.Table(self.events_name), p.Table(self.edges_name)
        await self.connection.execute(p.Insert(self.events).columns("account_id", "kind")
                                      .values(1, "click").values(1, "click").values(2, "click").values(2, "view"))
        await self.connection.execute(p.Insert(self.edges).columns("a", "b").values(1, 2).values(2, 3).values(3, 1))

    async def asyncTearDown(self):
        await self.context.__aexit__(None, None, None)
        await self.pool.close()

    async def rows(self, query):
        return [tuple(row.values()) for row in await self.connection.query(query)]

    def counting(self):
        return p.Select(int4(1).as_("n")).union_all(
            p.Select(p.col("n") + int4(1)).from_(p.Table("r")).where(p.col("n") < 5))

    async def test_columns_and_materialization(self):
        clicks = p.Select(p.col("account_id")).from_(self.events).where(p.col("kind") == "click")
        busy = p.Select(p.col("who")).from_(p.Table("clicks")).group_by(p.col("who")).having(
            p.call("count", p.col("who")) > 1)
        clause = p.With("clicks", clicks, columns=["who"], materialized=True).cte("busy", busy, materialized=False)
        query = p.Select().from_(p.Table("busy")).with_(clause)
        sql = query.inspect().sql
        self.assertIn('"clicks" ("who") AS MATERIALIZED (SELECT', sql)
        self.assertIn('"busy" AS NOT MATERIALIZED (SELECT', sql)
        self.assertEqual(await self.rows(query), [(1,)])

    async def test_recursive_search_orders_rows(self):
        for order, expected in ((p.SearchOrder.Breadth, [1, 2, 3, 4, 5]), (p.SearchOrder.Depth, [1, 2, 3, 4, 5])):
            with self.subTest(order=order):
                clause = p.With.recursive("r", self.counting(), columns=["n"],
                                          search=p.Search(order, p.col("n"), "ord"))
                query = p.Select(p.col("n")).from_(p.Table("r")).order_by(p.col("ord").asc()).with_(clause)
                self.assertEqual([row[0] for row in await self.rows(query)], expected)

    async def test_cycle_stops_at_a_repeated_row(self):
        e, walk = p.Table(self.edges_name, alias="e"), p.Table("walk")
        body = p.Select(p.col("a"), p.col("b")).from_(self.edges).where(p.col("a") == int4(1)).union_all(
            p.Select(e.col("a"), e.col("b")).from_(e).join(walk, e.col("a") == walk.col("b")))
        clause = p.With.recursive("walk", body, columns=("a", "b"),
                                  search=p.Search(p.SearchOrder.Depth, p.col("b"), "ord"),
                                  cycle=p.Cycle(p.col("b"), "looped", "path"))
        query = p.Select(p.col("b"), p.col("looped")).from_(walk).order_by(p.col("ord").asc()).with_(clause)
        self.assertEqual(await self.rows(query), [(2, False), (3, False), (1, False), (2, True)])

    async def test_recursive_clause_refusals(self):
        clause = p.With.recursive("r", self.counting())
        with self.assertRaises(p.ConstructionError):
            clause.cte("s", p.Select())
        target = p.Table(self.edges_name, alias="t")
        merge = p.merge(target, p.Table("r"), target.col("a") == p.col("n", table="r")).when_matched(
            p.MergeAction.delete())
        with self.assertRaises(p.ConstructionError):
            merge.with_(clause)
        plain = p.With("r", p.Select(p.col("a").as_("n")).from_(self.edges))
        self.assertEqual(await self.connection.execute(merge.with_(plain)), 3)
        with self.assertRaises(TypeError):
            p.With.recursive("r", p.Insert(self.edges).default_values())
        with self.assertRaises(TypeError):
            p.Search("depth", p.col("n"), "ord")


if __name__ == "__main__":
    unittest.main()
