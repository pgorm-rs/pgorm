"""MERGE and RETURNING's row versions against PostgreSQL, through the installed wheel."""

import os
import unittest
from uuid import uuid4

import pgorm as p


def quoted(name):
    return '"' + name.replace('"', '""') + '"'


# [spec:pgorm:req:python.statements+2/test]
class MergeTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
        self.context = self.pool.connection()
        self.connection = await self.context.__aenter__()
        suffix = uuid4().hex[:8]
        self.target_name, self.source_name = f'Merge "target" {suffix}', f"merge source {suffix}"
        await self.connection.execute(p.RawSQL(
            f"CREATE TEMP TABLE {quoted(self.target_name)} "
            "(id integer PRIMARY KEY, name text NOT NULL DEFAULT 'none', visits integer NOT NULL DEFAULT 0)"))
        await self.connection.execute(p.RawSQL(
            f"CREATE TEMP TABLE {quoted(self.source_name)} (id integer, name text, visits integer)"))
        self.target = p.Table(self.target_name).as_("t")
        self.source = p.Table(self.source_name).as_("s")
        await self.connection.execute(p.Insert(p.Table(self.target_name)).columns("id", "name", "visits")
                                      .values(1, "one", 10).values(2, "two", 20).values(3, "three", 30))
        await self.connection.execute(p.Insert(p.Table(self.source_name)).columns("id", "name", "visits")
                                      .values(1, "uno", 5).values(2, "dos", 50).values(4, "cuatro", 40))

    async def asyncTearDown(self):
        await self.context.__aexit__(None, None, None)
        await self.pool.close()

    def pending(self):
        t, s = self.target, self.source
        return p.merge(t, s, t.col("id") == s.col("id"))

    async def rows(self, query):
        return sorted(tuple(row.values()) for row in await self.connection.fetch_all(query))

    async def final(self):
        table = p.Table(self.target_name)
        return await self.rows(p.Select(p.col("id"), p.col("name"), p.col("visits")).from_(table))

    async def test_each_row_kind_takes_its_own_arm(self):
        t, s = self.target, self.source
        query = (self.pending()
                 .when_matched(p.MergeUpdate("name", s.col("name")).and_value("visits", t.col("visits") + s.col("visits")))
                 .when_not_matched(p.MergeInsert("id", s.col("id")).and_value("name", s.col("name"))
                                   .and_value("visits", s.col("visits")))
                 .when_not_matched_by_source(p.MatchedAction.Delete)
                 .returning_action()
                 .returning(t.col("id"), p.ReturningRow.Old.col("visits").as_("before"),
                            p.ReturningRow.New.col("visits").as_("after")))
        self.assertEqual(await self.rows(query), [
            ("DELETE", 3, 30, None), ("INSERT", 4, None, 40), ("UPDATE", 1, 10, 15), ("UPDATE", 2, 20, 70)])
        self.assertEqual(await self.final(), [(1, "uno", 15), (2, "dos", 70), (4, "cuatro", 40)])

    async def test_conditional_arms_precede_the_unconditional_arm(self):
        t, s = self.target, self.source
        # The unconditional arm is added first and still renders last; the first
        # conditional arm whose condition holds takes the row.
        query = (self.pending()
                 .when_matched(p.MergeUpdate("name", p.bind("fallback")))
                 .when_matched(p.MergeUpdate("name", p.bind("big")), condition=s.col("visits") > 10)
                 .when_matched(p.MatchedAction.Delete, condition=s.col("visits") > 1)
                 .when_not_matched(p.NotMatchedAction.DoNothing)
                 .returning_action().returning(t.col("id"), t.col("name")))
        sql = query.inspect().sql
        self.assertLess(sql.index("WHEN MATCHED AND"), sql.index("WHEN MATCHED THEN"))
        self.assertEqual(await self.rows(query), [("DELETE", 1, "one"), ("UPDATE", 2, "big")])
        self.assertEqual(await self.final(), [(2, "big", 20), (3, "three", 30)])

    async def test_last_unconditional_arm_wins(self):
        t, s = self.target, self.source
        query = (self.pending()
                 .when_matched(p.MatchedAction.Delete)
                 .when_matched(p.MatchedAction.DoNothing)
                 .when_not_matched(p.NotMatchedAction.InsertDefaultValues)
                 .returning_action().returning(t.col("name")))
        with self.assertRaises(p.DatabaseError) as caught:
            # The default row has no id: the target's primary key refuses it.
            await self.connection.fetch_all(query)
        self.assertEqual(caught.exception.sqlstate, "23502")
        query = (self.pending()
                 .when_matched(p.MatchedAction.Delete)
                 .when_matched(p.MatchedAction.DoNothing)
                 .when_not_matched(p.MergeInsert("id", s.col("id")))
                 .returning_action().returning(t.col("id"), t.col("name")))
        self.assertEqual(await self.rows(query), [("INSERT", 4, "none")])

    async def test_hostile_values_stay_bound(self):
        t, s = self.target, self.source
        hostile = "x'); DROP TABLE accounts; --"
        query = (self.pending()
                 .when_matched(p.MergeUpdate("name", hostile), condition=t.col("name") != hostile)
                 .returning(t.col("name")))
        self.assertEqual(await self.rows(query), [(hostile,), (hostile,)])
        self.assertNotIn("DROP", query.inspect().sql)

    async def test_cte_source_and_cte_body(self):
        t = self.target
        staged = p.Select(p.col("id"), p.col("visits")).from_(p.Table(self.source_name)).where_(p.col("visits") > 10)
        s = p.Table("staged")
        query = (p.merge(t, s, t.col("id") == s.col("id"))
                 .when_matched(p.MergeUpdate("visits", s.col("visits")))
                 .with_(p.With("staged", staged))
                 .returning_action().returning(t.col("id")))
        written = p.Table("written")
        outer = p.Select(written.star()).from_(written).with_(p.With("written", query))
        self.assertTrue(outer.inspect().sql.startswith('WITH "written" AS (WITH "staged" AS (SELECT'))
        self.assertEqual(await self.rows(outer), [("UPDATE", 2)])
        self.assertEqual(await self.final(), [(1, "one", 10), (2, "two", 50), (3, "three", 30)])

    async def test_renamed_versions_answer_only_to_their_names(self):
        t, s = self.target, self.source
        base = self.pending().when_matched(p.MergeUpdate("visits", s.col("visits")))
        query = base.returning(p.col("visits", table="before").as_("was"), p.col("visits", table="after").as_("is"),
                               old_as="before", new_as="after")
        self.assertEqual(await self.rows(query), [(10, 5), (20, 50)])
        with self.assertRaises(p.DatabaseError) as caught:
            await self.connection.fetch_all(base.returning(p.ReturningRow.Old.col("visits"), old_as="before"))
        self.assertEqual(caught.exception.sqlstate, "42P01")
        with self.assertRaises(p.DatabaseError) as caught:
            await self.connection.fetch_all(base.returning(old_as="s"))
        self.assertEqual(caught.exception.sqlstate, "42712")

    async def test_only_leaves_inheriting_tables_alone(self):
        parent, child = "merge parent " + uuid4().hex[:8], "merge child " + uuid4().hex[:8]
        await self.connection.execute(p.RawSQL(f"CREATE TEMP TABLE {quoted(parent)} (id integer, visits integer)"))
        await self.connection.execute(p.RawSQL(f"CREATE TEMP TABLE {quoted(child)} () INHERITS ({quoted(parent)})"))
        await self.connection.execute(p.Insert(p.Table(child)).columns("id", "visits").values(1, 0))
        target = p.Table(parent).as_("t")
        s = self.source
        query = p.merge(target, s, target.col("id") == s.col("id")).when_matched(p.MergeUpdate("visits", 99))
        self.assertEqual(await self.connection.execute(query.only()), 0)
        self.assertEqual(await self.connection.execute(query), 1)

    async def test_overriding_writes_an_always_identity_column(self):
        name = "merge identity " + uuid4().hex[:8]
        await self.connection.execute(p.RawSQL(
            f"CREATE TEMP TABLE {quoted(name)} (id integer GENERATED ALWAYS AS IDENTITY, n integer)"))
        target, s = p.Table(name).as_("t"), self.source
        insert = p.MergeInsert("id", s.col("id")).and_value("n", s.col("visits"))
        query = p.merge(target, s, target.col("id") == s.col("id"))
        with self.assertRaises(p.DatabaseError) as caught:
            await self.connection.execute(query.when_not_matched(insert))
        self.assertEqual(caught.exception.sqlstate, "428C9")
        written = query.when_not_matched(insert.overriding(p.Overriding.SystemValue)).returning(target.col("id"))
        self.assertEqual(await self.rows(written), [(1,), (2,), (4,)])

    async def test_actions_are_typed_by_their_row(self):
        pending = self.pending()
        with self.assertRaises(TypeError):
            pending.when_matched(p.MergeInsert("id", 1))
        with self.assertRaises(TypeError):
            pending.when_not_matched_by_source(p.NotMatchedAction.DoNothing)
        with self.assertRaises(TypeError):
            pending.when_not_matched(p.MergeUpdate("id", 1))
        with self.assertRaises(TypeError):
            pending.when_not_matched(p.MatchedAction.Delete)
        with self.assertRaises(p.ConstructionError):
            await self.connection.execute(pending)
        self.assertFalse(hasattr(pending, "inspect"))


# [spec:pgorm:req:python.statements+2/test]
class ReturningVersionTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
        self.context = self.pool.connection()
        self.connection = await self.context.__aenter__()
        self.name = "versions " + uuid4().hex[:8]
        await self.connection.execute(p.RawSQL(
            f"CREATE TEMP TABLE {quoted(self.name)} (id integer PRIMARY KEY, visits integer NOT NULL)"))
        self.table = p.Table(self.name)
        await self.connection.execute(p.Insert(self.table).columns("id", "visits").values(1, 10))

    async def asyncTearDown(self):
        await self.context.__aexit__(None, None, None)
        await self.pool.close()

    async def test_update_and_delete_read_both_versions(self):
        old, new = p.ReturningRow.Old, p.ReturningRow.New
        update = (p.Update(self.table).set("visits", p.col("visits") + 5).where_(p.col("id") == 1)
                  .returning(old.col("visits").as_("before"), new.col("visits").as_("after")))
        self.assertEqual(dict(await self.connection.fetch_one(update)), {"before": 10, "after": 15})
        delete = p.Delete(self.table).where_(p.col("id") == 1).returning(old.star(), new.col("id").as_("gone"))
        self.assertEqual(dict(await self.connection.fetch_one(delete)), {"id": 1, "visits": 15, "gone": None})

    async def test_upsert_says_which_rows_it_inserted(self):
        old = p.ReturningRow.Old
        upsert = (p.Insert(self.table).columns("id", "visits").values(1, 1).values(2, 2)
                  .on_conflict(p.ConflictTarget("id").set("visits", p.col("visits", table="excluded")))
                  .returning(p.col("id"), old.col("id").is_null().as_("inserted")))
        rows = sorted(tuple(row.values()) for row in await self.connection.fetch_all(upsert))
        self.assertEqual(rows, [(1, False), (2, True)])

    async def test_renamed_versions_on_an_update(self):
        update = (p.Update(self.table).set("visits", 0).where_(p.col("id") == 1)
                  .returning(p.col("visits", table="o").as_("was"), p.col("visits", table="n").as_("is"),
                             old_as="o", new_as="n"))
        self.assertEqual(tuple((await self.connection.fetch_one(update)).values()), (10, 0))
        same = p.Update(self.table).set("visits", 0).all_rows().returning(old_as="o", new_as="o")
        with self.assertRaises(p.DatabaseError) as caught:
            await self.connection.fetch_all(same)
        self.assertEqual(caught.exception.sqlstate, "42712")
