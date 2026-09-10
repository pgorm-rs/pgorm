"""Installed Python pipeline composition, scope ownership and PostgreSQL results."""

import os
import unittest

import pgorm as p
from pgorm import pipeline as pl
from pgorm import _native
from pipeline_programs import programs


class PipelineConstruction(unittest.TestCase):
    # [spec:pgorm:req:python.pipeline/test]
    def test_public_programs_compile_with_native_parameters(self):
        cases = programs(_native)
        self.assertEqual(len(cases), 21)
        for name, query in cases.items():
            with self.subTest(name=name):
                self.assertIsInstance(query.inspect(), p.Compiled)
        query = cases["repeated"].inspect()
        self.assertEqual([(v.kind, v.value) for v in query.params], [("i32", 2)])
        self.assertEqual(query.sql.count("$1"), 2)
        self.assertEqual(cases["append"].inspect().params[1].value, 8)
        self.assertEqual(
            p.capabilities()["pipeline_policy"]["source_arities"], list(range(1, 7))
        )
        self.assertEqual(p.capabilities()["registrations"]["sources"], [])
        with self.assertRaises(p.UnsupportedCapabilityError):
            pl.sources("unregistered")

    # [spec:pgorm:req:python.pipeline/test]
    def test_scopes_close_on_success_and_exceptions(self):
        base = pl.from_(p.Table("items"))
        key = pl.col("items", "id")
        saved = []

        def callback(binder):
            value = binder.bind(2)
            saved.extend([binder, value])
            return key > value

        query = base.filter_with(callback)
        self.assertEqual(query.inspect().params[0].value, 2)
        binder, value = saved
        for operation in (
            lambda: binder.bind(3),
            lambda: value + 1,
            lambda: base.filter(value),
            lambda: base.filter_with(lambda b: value),
            lambda: pl.over().by(value),
        ):
            with self.assertRaises(p.LifecycleError):
                operation()
        saved.clear()

        def failing(binder):
            saved.append(binder)
            raise ValueError("callback failure")

        with self.assertRaisesRegex(ValueError, "callback failure"):
            base.filter_with(failing)
        with self.assertRaises(p.LifecycleError):
            saved[0].bind(1)
        self.assertEqual(base.inspect().params, [])

    # [spec:pgorm:req:python.pipeline/test]
    def test_nested_callbacks_cannot_mix_brands(self):
        base = pl.from_(p.Table("items"))
        key = pl.col("items", "id")

        def outer(binder):
            value = binder.bind(1)
            with self.assertRaises(p.LifecycleError):
                base.filter_with(lambda inner: (key > value) & (key < inner.bind(3)))
            with self.assertRaises(p.LifecycleError):
                base.filter_with(lambda inner: value)
            with self.assertRaises(p.LifecycleError):
                pl.over().sort_by(value)
            independent = base.filter_with(lambda inner: key < inner.bind(9))
            self.assertEqual(independent.inspect().params[0].value, 9)
            return key > value

        self.assertEqual(base.filter_with(outer).inspect().params[0].value, 1)

    # [spec:pgorm:req:python.pipeline/test]
    def test_bound_list_limits_and_input_rejections(self):
        base = pl.from_(p.Table("items"))
        self.assertIsInstance(base.derive_with(lambda b: []), pl.Pipeline)
        full = base.select_with(lambda b: [b.bind(i).as_(f"x{i}") for i in range(32)])
        self.assertEqual(len(full.inspect().params), 32)
        with self.assertRaises(p.UnsupportedCapabilityError):
            base.select_with(lambda b: [b.bind(i) for i in range(33)])
        self.assertIsInstance(
            base.select(*(pl.literal(i).as_(f"x{i}") for i in range(33))).inspect(),
            p.Compiled,
        )
        for operation in (
            lambda: base.take(True),
            lambda: base.take(2**64),
            lambda: pl.alias(""),
            lambda: pl.col("items", "bad\0name"),
            lambda: base.select_with(lambda b: "bad"),
            lambda: pl.literal(pl.col("items", "id")),
            lambda: bool(base),
            lambda: bool(pl.col("items", "id")),
            lambda: bool(base.group(pl.col("items", "id"))),
        ):
            with self.assertRaises(p.ConstructionError):
                operation()
        for value in (float("inf"), p.Value(2, "i16"), p.Value.null("i32")):
            with self.assertRaises(p.UnsupportedCapabilityError):
                pl.literal(value)
        with self.assertRaises(p.UnsupportedCapabilityError):
            base.filter_with(
                lambda b: b.bind(p.Value("calm", p.TypeName("Mood", schema="app")))
            )
        with self.assertRaises(p.ConstructionError):
            base.select(pl.literal(1).as_("sum")).inspect()


class PipelineDatabase(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
        self.connection = await self.pool.acquire()
        await self.connection.execute(
            p.RawSQL(
                "CREATE TEMP TABLE items (id integer, category text, amount bigint)"
            )
        )
        await self.connection.execute(
            p.RawSQL("INSERT INTO items VALUES (1,'a',2),(2,'a',3),(3,'b',4),(4,'b',5)")
        )

    async def asyncTearDown(self):
        await self.connection.close()
        await self.pool.close()

    # [spec:pgorm:req:python.pipeline/test]
    async def test_terminals_models_and_dynamic_streams(self):
        model = p.Model(
            "items",
            {
                "id": p.Column("i32"),
                "category": p.Column("text"),
                "amount": p.Column("i64"),
            },
        )
        query = pl.from_(model).sort(pl.col("items", "id"))
        self.assertEqual((await query.one(self.connection))["id"], 1)
        self.assertIn("LIMIT 1", query.inspect(terminal="one").sql)
        with self.assertRaises(p.DatabaseError):
            await self.connection.fetch_one(query)
        empty = query.filter(pl.col("items", "id") < 0)
        self.assertIsNone(await empty.one_opt(self.connection))
        with self.assertRaises(p.DatabaseError):
            await empty.one(self.connection)
        async with await self.connection.stream(query) as rows:
            found = [row["id"] async for row in rows]
        self.assertEqual(found, [1, 2, 3, 4])

    # [spec:pgorm:req:python.pipeline/test]
    async def test_pipeline_stages_execute_in_postgresql(self):
        cases = programs(_native)
        expected = {
            "literal": [4, 3],
            "repeated": [3, 4],
            "literal_string": [],
            "in_array": [1, 2],
            "sort_with": [2, 3, 4],
        }
        for name, identifiers in expected.items():
            with self.subTest(name=name):
                rows = await cases[name].all(self.connection)
                self.assertEqual([row["id"] for row in rows], identifiers)
        self.assertEqual(
            sorted(
                (r["category"], r["total"])
                for r in await cases["group"].all(self.connection)
            ),
            [("a", 5), ("b", 9)],
        )
        self.assertEqual(
            [r["answer"] for r in await cases["case"].all(self.connection)],
            ["no", "yes", "yes", "yes"],
        )
        for name in (
            "derive",
            "derive_with",
            "select_with",
            "group_with",
            "window",
            "window_with",
            "append",
            "intersect",
            "remove",
            "distinct",
            "cast_unary",
            "join",
        ):
            with self.subTest(name=name):
                await cases[name].all(self.connection)

    # [spec:pgorm:req:python.pipeline/test]
    async def test_literal_and_bound_text_remain_values(self):
        payload = "O'Brien'); DROP TABLE items; -- 雪"
        await self.connection.execute(
            p.Insert(p.Table("items"))
            .columns("id", "category", "amount")
            .values(5, payload, 6)
        )
        base = pl.from_(p.Table("items"))
        key = pl.col("items", "category")
        literal = base.filter(key == payload)
        bound = base.filter_with(lambda b: key == b.bind(payload))
        self.assertEqual(literal.inspect().params, [])
        self.assertNotIn(payload, bound.inspect().sql)
        for query in (literal, bound):
            row = await query.one(self.connection)
            self.assertEqual((row["id"], row["category"]), (5, payload))
        self.assertEqual(len(await base.all(self.connection)), 5)

    # [spec:pgorm:req:python.pipeline/test]
    async def test_aggregates_windows_and_typed_nulls(self):
        base = pl.from_(p.Table("items"))
        amount = pl.col("items", "amount")
        category = pl.col("items", "category")
        aggregates = {
            "sum": (pl.sum(amount), 14),
            "min": (pl.min(amount), 2),
            "max": (pl.max(amount), 5),
            "average": (pl.average(amount), 3.5),
            "count": (pl.count(amount), 4),
            "count_rows": (pl.count_rows(), 4),
            "count_distinct": (pl.count_distinct(category), 2),
        }
        for name, (expression, expected) in aggregates.items():
            with self.subTest(name=name):
                row = (
                    await base.group()
                    .aggregate(expression.as_("result"))
                    .one(self.connection)
                )
                self.assertEqual(row["result"], expected)
        for name, expression in {
            "rank": pl.rank(amount),
            "rank_dense": pl.rank_dense(amount),
            "first": pl.first(amount),
            "last": pl.last(amount),
            "lag": pl.lag(1, amount),
            "lead": pl.lead(1, amount),
            "stddev": pl.stddev(amount),
        }.items():
            with self.subTest(name=name):
                query = base.window(
                    expression.as_("result"),
                    over=pl.over().sort_by(amount).range(None, 0),
                )
                self.assertEqual(len(await query.all(self.connection)), 4)
        query = base.select_with(
            lambda b: [b.bind(p.Value.null("i32")).cast("integer").as_("result")]
        ).take(1)
        self.assertEqual(query.inspect().params[0].kind, "i32")
        self.assertIsNone((await query.one(self.connection))["result"])


if __name__ == "__main__":
    unittest.main()
