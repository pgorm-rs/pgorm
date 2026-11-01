"""Registered entities, graphs and source selections run on a pool, in the application wheel."""

import asyncio
import os
import unittest

import pgorm as p
from pgorm import pipeline as pl, schema as s


# [spec:pgorm:req:bindings.registered-pools/test]
class RegisteredPools(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        # One connection, so an operation that kept the pool's connection
        # would leave the next one waiting out the acquire timeout.
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1, acquire_timeout=5)
        self.account = p.entity("app.Account")
        self.note = p.entity("app.Note")
        await self.pool.execute(p.RawSQL("CREATE SCHEMA python_entities"))
        for entity in (self.account, self.note):
            generated = s.from_entity(entity)
            for enum in generated.enums:
                await self.pool.execute(enum)
            await self.pool.execute(generated.table)

    async def asyncTearDown(self):
        await self.pool.execute(p.RawSQL("DROP SCHEMA python_entities CASCADE"))
        await self.pool.close()

    def active(self, identity, name="Nora"):
        return self.account.active().set("id", identity).set("display name", name).set("note", None)

    def assert_returned(self):
        """The pool's one connection is back in it, idle."""
        status = self.pool.status()
        self.assertEqual((status.size, status.available), (1, 1))

    async def test_entity_reads_and_writes_run_on_pools(self):
        model = await self.active(1).insert(self.pool)
        self.assert_returned()
        self.assertEqual(model["display name"], "Nora|before")
        query = self.account.find()
        self.assertEqual([row["id"] for row in await query.all(self.pool)], [1])
        self.assertEqual((await query.one(self.pool))["id"], 1)
        self.assertIsNone(await query.where(self.account.col("id") == 2).optional(self.pool))
        self.assert_returned()
        updated = await model.into_active().set("note", "pooled").update(self.pool)
        self.assertEqual((updated["note"], updated["version"]), ("pooled", 2))
        self.assertEqual(await updated.into_active().delete(self.pool), 1)
        self.assert_returned()
        self.assertEqual(await query.all(self.pool), [])

    async def test_version_terminals_run_on_a_pool(self):
        model = await self.active(1).insert(self.pool)
        change = await self.account.update(model.into_active().set("note", "one")).returning_change(self.pool)
        self.assertEqual((change.old["note"], change.new["note"]), (None, "one"))
        changes = await self.account.update_many().set("note", "all").all_rows().returning_changes(self.pool)
        self.assertEqual([(c.old["note"], c.new["note"]) for c in changes], [("one", "all")])
        renamed = p.ConflictTarget("id").update("display name")
        upsert = await self.account.insert(self.active(1, "again")).on_conflict(renamed).returning_upsert(self.pool)
        self.assertIsInstance(upsert, p.Upserted.Updated)
        upserts = await (self.account.insert_many([self.active(1, "third"), self.active(2, "new")])
                         .on_conflict(renamed).returning_upserts(self.pool))
        self.assertEqual([isinstance(row, p.Upserted.Updated) for row in upserts], [True, False])
        self.assert_returned()

    async def test_graph_and_source_terminals_run_on_pools(self):
        await self.active(1).insert(self.pool)
        await self.active(2).insert(self.pool)
        await self.note.active().set("id", 11).set("account_id", 1).set("body", "first").insert(self.pool)
        graph = p.graph("app.AccountNotes").find()
        ordered = graph.order_by(graph.col(0, "id").asc())
        self.assertEqual([(a["id"], None if n is None else n["id"]) for a, n in await ordered.all(self.pool)],
                         [(1, 11), (2, None)])
        self.assertEqual((await ordered.optional(self.pool))[0]["id"], 1)
        page = await graph.cursor("id").after(1).first(1).all(self.pool)
        self.assertEqual([(a["id"], n) for a, n in page], [(2, None)])
        self.assert_returned()
        selected = (pl.from_(self.account).filter(pl.col("accounts", "id") == 1)
                    .select_sources(pl.sources("app.SingleAccount")))
        self.assertEqual([row[0]["id"] for row in await selected.all(self.pool)], [1])
        self.assertEqual((await selected.one(self.pool))[0]["id"], 1)
        self.assertEqual((await selected.optional(self.pool))[0]["id"], 1)
        self.assert_returned()

    async def test_each_operation_borrows_its_own_connection(self):
        pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=2, acquire_timeout=5)
        try:
            async with pool.transaction() as tx:
                await self.active(1).insert(tx)
                self.assertEqual(len(await self.account.find().all(tx)), 1)
                # The pool's operation runs outside the open transaction, so
                # its uncommitted row is not there to read.
                self.assertEqual(await self.account.find().all(pool), [])
            self.assertEqual(len(await self.account.find().all(pool)), 1)
        finally:
            await pool.close()

    # [spec:pgorm:req:bindings.cancellation/test]
    async def test_a_cancelled_pool_operation_discards_its_connection(self):
        task = asyncio.ensure_future(self.active(1, "wait_before").insert(self.pool))
        await asyncio.sleep(0.05)
        task.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await asyncio.wait_for(task, 1)
        self.assertTrue(await asyncio.wait_for(self.pool.ping(), 5))
        self.assertEqual(await self.account.find().all(self.pool), [])

    # [spec:pgorm:req:bindings.argument-errors/test]
    async def test_anything_but_an_open_executor_is_refused(self):
        query = self.account.find()
        for other in (None, "pool", self.account):
            with self.subTest(other=other), self.assertRaises(TypeError):
                await query.all(other)
        closed = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
        await closed.close()
        with self.assertRaises(p.LifecycleError):
            await query.all(closed)
        with self.assertRaises(p.LifecycleError):
            await self.active(1).insert(closed)


if __name__ == "__main__":
    unittest.main()
