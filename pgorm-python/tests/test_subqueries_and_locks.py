"""A SELECT read as a FROM item, joined LATERAL, and the rows a SELECT locks,
against PostgreSQL through the installed wheel."""

import os
import unittest
from uuid import uuid4

import pgorm as p

S, W = p.LockStrength, p.LockWait


def quoted(name):
    return '"' + name.replace('"', '""') + '"'


class ReadTests(unittest.IsolatedAsyncioTestCase):
    """Two real tables, so a second connection sees the rows a first one locks."""

    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=2)
        suffix = uuid4().hex[:8]
        self.names = (f'Reads "users" {suffix}', f"reads orders {suffix}")
        users, orders = map(quoted, self.names)
        await self.pool.execute(p.RawSQL(f"CREATE TABLE {users} (id int PRIMARY KEY, name text NOT NULL)"))
        await self.pool.execute(p.RawSQL(
            f"CREATE TABLE {orders} (id int PRIMARY KEY, user_id int NOT NULL REFERENCES {users}, total int)"))
        await self.pool.execute(p.Insert(p.Table(self.names[0])).columns("id", "name")
                                .values(1, "Ann").values(2, "Bob").values(3, "Cy"))
        await self.pool.execute(p.Insert(p.Table(self.names[1])).columns("id", "user_id", "total")
                                .values(1, 1, 10).values(2, 1, 30).values(3, 2, 20))
        self.u = p.Table(self.names[0], alias="u")
        self.o = p.Table(self.names[1], alias="o")

    async def asyncTearDown(self):
        users, orders = map(quoted, self.names)
        await self.pool.execute(p.RawSQL(f"DROP TABLE IF EXISTS {orders}, {users}"))
        await self.pool.close()

    async def rows(self, query, target=None):
        return [tuple(row.values()) for row in await (target or self.pool).query(query)]


# [spec:pgorm:req:bindings.from-subqueries/test]
class FromSubqueryTests(ReadTests):
    def totals(self):
        o = self.o
        return (p.Select(o.col("user_id"), p.call("sum", o.col("total")).as_("spent"))
                .from_(o).group_by(o.col("user_id")).as_("t"))

    async def test_subquery_reads_in_from_and_joins(self):
        u, t = self.u, self.totals()
        joined = (p.Select(u.col("name"), t.col("spent")).from_(u)
                  .join(t, t.col("user_id") == u.col("id")).order_by(u.col("id").asc()))
        self.assertEqual(await self.rows(joined), [("Ann", 40), ("Bob", 20)])
        listed = (p.Select(u.col("name"), t.col("spent")).from_(u).from_(t)
                  .where(t.col("user_id") == u.col("id")).order_by(t.col("spent").desc()))
        self.assertEqual(await self.rows(listed), [("Ann", 40), ("Bob", 20)])
        outer = p.Select(t.star()).from_(t).order_by(t.col("user_id").asc())
        self.assertEqual(await self.rows(outer), [(1, 40), (2, 20)])

    async def test_lateral_subquery_reads_earlier_items(self):
        u, o = self.u, self.o
        latest = (p.Select(o.col("total")).from_(o).where(o.col("user_id") == u.col("id"))
                  .order_by(o.col("total").desc()).limit(1).as_("l"))
        query = (p.Select(u.col("name"), latest.col("total")).from_(u)
                 .join(latest, p.literal(True), kind=p.Join.Left, lateral=True).order_by(u.col("id").asc()))
        self.assertIn("LEFT JOIN LATERAL (SELECT", query.inspect().sql)
        self.assertEqual(await self.rows(query), [("Ann", 30), ("Bob", 20), ("Cy", None)])
        plain = p.Select(u.col("name")).from_(u).join(latest, p.literal(True), kind=p.Join.Left)
        with self.assertRaises(p.DatabaseError) as caught:
            await self.rows(plain)
        self.assertEqual(caught.exception.sqlstate, "42P01")

    async def test_unreadable_subquery_is_refused(self):
        o = self.o
        with self.assertRaises(p.ConstructionError):
            p.Select(o.col("id")).from_(o).limit_with_ties(1).as_("x")
        with self.assertRaises(p.ConstructionError):
            p.Select().from_(self.u).join(o, p.literal(True), lateral=True)
        with self.assertRaises(TypeError):
            p.Select().from_(self.u).join(self.totals(), p.literal(True), lateral="yes")


# [spec:pgorm:req:bindings.row-locks/test]
class RowLockTests(ReadTests):
    async def test_strengths_conflict_as_postgresql_orders_them(self):
        u = self.u
        first = p.Select(u.col("id")).from_(u).where(u.col("id") == 1)
        async with self.pool.transaction() as holder:
            await holder.query(first.lock(S.NoKeyUpdate))
            async with self.pool.connection() as other:
                self.assertEqual(await self.rows(first.lock(S.KeyShare, wait=W.Nowait), other), [(1,)])
                for strength in (S.Share, S.NoKeyUpdate, S.Update):
                    with self.subTest(strength=strength), self.assertRaises(p.DatabaseError) as caught:
                        await other.query(first.lock(strength, wait=W.Nowait))
                    self.assertEqual(caught.exception.sqlstate, "55P03")
                every = p.Select(u.col("id")).from_(u).order_by(u.col("id").asc())
                self.assertEqual(await self.rows(every.lock(S.Update, wait=W.SkipLocked), other), [(2,), (3,)])

    async def test_of_locks_only_the_items_named(self):
        u, o = self.u, self.o
        async with self.pool.transaction() as holder:
            await holder.query(p.Select(u.col("id")).from_(u).where(u.col("id") == 1).lock(S.Update))
            async with self.pool.connection() as other:
                joined = (p.Select(o.col("id")).from_(o).join(u, u.col("id") == o.col("user_id"))
                          .where(u.col("id") == 1).order_by(o.col("id").asc()))
                self.assertEqual(await self.rows(joined.lock(S.Update, of=[o], wait=W.Nowait), other), [(1,), (2,)])
                with self.assertRaises(p.DatabaseError) as caught:
                    await other.query(joined.lock(S.Update, of=[u], wait=W.Nowait))
                self.assertEqual(caught.exception.sqlstate, "55P03")
                self.assertEqual(await self.rows(joined.lock(S.Update, of=[u], wait=W.SkipLocked), other), [])

    async def test_lock_names_items_as_they_answer(self):
        u = self.u
        sql = p.Select(u.col("id")).from_(u).lock(S.Share, of=[u, p.Table("plain")]).inspect().sql
        self.assertTrue(sql.endswith('FOR SHARE OF "u", "plain"'), sql)
        with self.assertRaises(p.DatabaseError) as caught:
            await self.pool.query(p.Select(u.col("id")).from_(u).lock(S.Share, of=[p.Table(self.names[0])]))
        self.assertEqual(caught.exception.sqlstate, "42P01")
        with self.assertRaises(TypeError):
            p.Select().from_(u).lock("update")
        with self.assertRaises(TypeError):
            p.Select().from_(u).lock(S.Update, wait="nowait")


if __name__ == "__main__":
    unittest.main()
