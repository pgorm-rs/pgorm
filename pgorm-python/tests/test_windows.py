"""Window functions over a runtime Select against PostgreSQL, through the installed wheel."""

import datetime
import decimal
import os
import unittest
from uuid import uuid4

import pgorm as p

READINGS = [
    (1, datetime.datetime(2026, 1, 1, 0, 0), decimal.Decimal("1.0"), 1, 1),
    (2, datetime.datetime(2026, 1, 1, 12, 0), decimal.Decimal("1.4"), 2, 2),
    (3, datetime.datetime(2026, 1, 2, 6, 0), decimal.Decimal("2.0"), 2, 4),
    (4, datetime.datetime(2026, 1, 4, 0, 0), decimal.Decimal("3.5"), 3, 8),
]
F = p.FrameType


def quoted(name):
    return '"' + name.replace('"', '""') + '"'


# [spec:pgorm:req:python.statements+3/test]
class WindowTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
        self.context = self.pool.connection()
        self.connection = await self.context.__aenter__()
        self.name = f'Window "reading" {uuid4().hex[:8]}'
        await self.connection.execute(p.RawSQL(
            f"CREATE TEMP TABLE {quoted(self.name)} (id int PRIMARY KEY, at timestamp NOT NULL, "
            "level numeric NOT NULL, grade int NOT NULL, weight int NOT NULL, kind text)"))
        insert = p.Insert(p.Table(self.name)).columns("id", "at", "level", "grade", "weight", "kind")
        for row in READINGS:
            insert = insert.values(*row, "even" if row[0] % 2 == 0 else "odd")
        await self.connection.execute(insert)
        self.reading = p.Table(self.name).as_("r")

    async def asyncTearDown(self):
        await self.context.__aexit__(None, None, None)
        await self.pool.close()

    def r(self, name):
        return self.reading.col(name)

    async def column(self, item):
        query = p.Select(item).from_(self.reading).order_by(self.r("id").asc())
        return [next(iter(row.values())) for row in await self.connection.fetch_all(query)]

    async def sums(self, by, frame):
        window = p.Window().order_by(self.r(by).asc()).frame(frame)
        return await self.column(p.call("sum", self.r("weight")).over(window))

    async def sqlstate(self, item):
        with self.assertRaises(p.DatabaseError) as caught:
            await self.column(item)
        return caught.exception.sqlstate

    async def test_aggregate_over_partition_and_ordering(self):
        by_kind = p.Window().partition_by(self.r("kind"))
        running = p.Window().partition_by(self.r("kind")).order_by(self.r("id").asc())
        self.assertEqual(await self.column(p.call("count", self.r("id")).over(by_kind)), [2, 2, 2, 2])
        self.assertEqual(await self.column(p.call("sum", self.r("weight")).over(running)), [1, 2, 5, 10])
        self.assertEqual(await self.column(p.call("sum", self.r("weight")).over(p.Window())),
                         [15, 15, 15, 15])

    async def test_window_functions_number_and_reach_rows(self):
        ordered = p.Window().order_by(self.r("grade").asc())
        by_id = p.Window().order_by(self.r("id").asc())
        self.assertEqual(await self.column(p.window_function("row_number").over(by_id)), [1, 2, 3, 4])
        self.assertEqual(await self.column(p.window_function("rank").over(ordered)), [1, 2, 2, 4])
        self.assertEqual(await self.column(p.window_function("dense_rank").over(ordered)), [1, 2, 2, 3])
        self.assertEqual(await self.column(
            p.window_function("lag", self.r("weight"), p.literal(1), p.literal(0)).over(by_id)), [0, 1, 2, 4])
        self.assertEqual(await self.column(
            p.window_function("nth_value", self.r("weight"), p.literal(2)).over(
                by_id.frame(F.Rows.unbounded_preceding().and_unbounded_following()))), [2, 2, 2, 2])

    async def test_frames_follow_each_mode(self):
        one_day = p.literal("1 day").cast(p.TypeName("interval"))
        self.assertEqual(await self.sums("at", F.Range.preceding(one_day).and_current_row()), [1, 3, 6, 8])
        self.assertEqual(await self.sums("at", F.Rows.preceding(1).and_current_row()), [1, 3, 6, 12])
        half = decimal.Decimal("0.5")
        self.assertEqual(await self.sums("level", F.Range.preceding(half).and_following(half)), [3, 3, 4, 8])
        self.assertEqual(await self.sums("grade", F.Groups.preceding(1).and_current_row()), [1, 7, 7, 14])
        self.assertEqual(await self.sums("id", F.Rows.unbounded_preceding()), [1, 3, 7, 15])
        self.assertEqual(await self.sums("id", F.Rows.current_row().and_following(1)), [3, 6, 12, 8])
        self.assertEqual(await self.sums("id", F.Rows.following(1).and_unbounded_following()),
                         [14, 12, 8, None])

    async def test_each_exclusion_removes_its_own_rows(self):
        whole = F.Rows.unbounded_preceding().and_unbounded_following()
        X = p.FrameExclusion
        self.assertEqual(await self.sums("grade", whole.exclude(X.NoOthers)), [15, 15, 15, 15])
        self.assertEqual(await self.sums("grade", whole.exclude(X.CurrentRow)), [14, 13, 11, 7])
        self.assertEqual(await self.sums("grade", whole.exclude(X.Group)), [14, 9, 9, 7])
        self.assertEqual(await self.sums("grade", whole.exclude(X.Ties)), [15, 11, 13, 15])
        self.assertEqual(await self.sums("grade", F.Groups.preceding(1).and_current_row()
                                         .exclude(X.CurrentRow)), [None, 5, 3, 6])
        self.assertEqual(await self.sums("id", F.Rows.current_row().exclude(X.CurrentRow)),
                         [None, None, None, None])

    async def test_named_window_is_read_by_name(self):
        window = p.Window().partition_by(self.r("kind")).order_by(self.r("id").asc())
        query = (p.Select(self.r("id"), p.call("sum", self.r("weight")).over("Running W").as_("Total"),
                          p.window_function("row_number").over("Running W").as_("n"))
                 .from_(self.reading).window("Running W", window).order_by(self.r("id").asc()))
        self.assertIn('OVER "Running W" AS "Total"', query.inspect().sql)
        self.assertIn('WINDOW "Running W" AS', query.inspect().sql)
        rows = [tuple(row.values()) for row in await self.connection.fetch_all(query)]
        self.assertEqual(rows, [(1, 1, 1), (2, 2, 1), (3, 5, 2), (4, 10, 2)])
        unknown = p.Select(p.call("sum", self.r("weight")).over("running w")).from_(self.reading).window(
            "Running W", window)
        with self.assertRaises(p.DatabaseError) as caught:
            await self.connection.fetch_all(unknown)
        self.assertEqual(caught.exception.sqlstate, "42704")

    async def test_json_aggregates_run_as_window_functions(self):
        by_id = p.Window().order_by(self.r("id").asc())
        self.assertEqual(await self.column(
            p.json_arrayagg(self.r("weight"), returning="jsonb").over(by_id)),
            [[1], [1, 2], [1, 2, 4], [1, 2, 4, 8]])
        # Within a kind the later reading's weight replaces the earlier one's.
        self.assertEqual(await self.column(
            p.json_objectagg(self.r("kind"), self.r("weight"), returning="jsonb").over(
                p.Window().partition_by(self.r("kind")).order_by(self.r("id").asc()))),
            [{"odd": 1}, {"even": 2}, {"odd": 4}, {"even": 8}])
        self.assertEqual(await self.column(
            p.json_arrayagg(self.r("weight"), returning="jsonb", filter=self.r("weight") > 1).over(by_id)),
            [None, [2], [2, 4], [2, 4, 8]])

    async def test_server_refuses_what_builder_cannot_see(self):
        offset = F.Range.preceding(1).and_current_row()
        self.assertEqual(await self.sqlstate(p.call("sum", self.r("weight")).over(
            p.Window().frame(offset))), "42P20")
        by_time = p.Window().order_by(self.r("at").asc())
        self.assertEqual(await self.sqlstate(p.call("sum", self.r("weight")).over(
            by_time.frame(F.Range.preceding(p.literal(1)).and_current_row()))), "0A000")
        # Bound, the offset is a parameter the server types interval from the
        # pairing, and an integer cannot be written as one.
        with self.assertRaises(p.ConstructionError):
            await self.column(p.call("sum", self.r("weight")).over(by_time.frame(offset)))
        self.assertEqual(await self.sqlstate(p.call("count_distinct", self.r("id")).over(p.Window())), "0A000")

    def test_over_needs_call_and_frame_valid_start(self):
        window = p.Window()
        for item in (p.col("weight"), p.col("weight") + 1, p.literal(1), p.json_value(p.col("doc"), "$.a")):
            with self.assertRaises(p.ConstructionError):
                item.over(window)
        with self.assertRaises(p.ConstructionError):
            window.frame(F.Rows.following(1))
        with self.assertRaises(AttributeError):
            getattr(F.Rows.current_row(), "and_preceding")
        with self.assertRaises(AttributeError):
            getattr(F.Rows.following(1), "and_current_row")
        with self.assertRaises(AttributeError):
            getattr(F.Rows, "unbounded_following")
        with self.assertRaises(AttributeError):
            getattr(window, "exclude")
        with self.assertRaises(p.UnsupportedCapabilityError):
            p.window_function("row_number", p.col("id"))
        with self.assertRaises(p.UnsupportedCapabilityError):
            p.window_function("lag")
        with self.assertRaises(AttributeError):
            getattr(p.window_function("rank"), "eq")
