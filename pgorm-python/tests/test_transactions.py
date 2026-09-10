"""Native borrowed transactions, savepoints and bounded cancellation."""
import asyncio
import gc
import os
import time
import unittest

import pgorm as p
from pgorm import schema as s


class Transactions(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=3)
        self.table = p.Table("python_transaction_items")
        await self.pool.execute(s.create_table(self.table).column(s.ColumnDef("id", "integer").primary_key()).column(s.ColumnDef("name", "text")))

    async def asyncTearDown(self):
        if self.pool.closed:
            self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=3)
        await self.pool.execute(s.drop_table(self.table, if_exists=True))
        await self.pool.close()

    def insert(self, identity):
        return p.insert(self.table).columns("id", "name").values(identity, f"item {identity}")

    def rows(self):
        return p.select(p.col("id")).from_(self.table).order_by(p.col("id").asc())

    # [spec:pgorm:req:python.transactions/test]
    async def test_explicit_commit_rollback_and_parent_reservation(self):
        async with self.pool.connection() as connection:
            tx = await connection.begin()
            self.assertFalse(tx.closed)
            await tx.execute(self.insert(1))
            self.assertEqual([r["id"] for r in await tx.fetch_all(self.rows())], [1])
            self.assertEqual(await self.pool.fetch_all(self.rows()), [])
            with self.assertRaises(p.LifecycleError):
                await connection.execute(self.insert(2))
            await tx.commit()
            self.assertTrue(tx.closed)
            self.assertTrue(await connection.ping())
            with self.assertRaises(p.LifecycleError):
                await tx.rollback()
            await tx.close()
            tx = await connection.begin()
            await tx.execute(self.insert(2))
            await tx.rollback()
            self.assertEqual([r["id"] for r in await connection.fetch_all(self.rows())], [1])

    # [spec:pgorm:req:python.transactions/test]
    async def test_contexts_commit_and_preserve_original_exception(self):
        async with self.pool.transaction() as tx:
            await tx.execute(self.insert(1))
        self.assertTrue(tx.closed)
        marker = ValueError("application error")
        with self.assertRaises(ValueError) as caught:
            async with self.pool.transaction() as tx:
                await tx.execute(self.insert(2))
                raise marker
        self.assertIs(caught.exception, marker)
        self.assertTrue(tx.closed)
        self.assertEqual([r["id"] for r in await self.pool.fetch_all(self.rows())], [1])

    # [spec:pgorm:req:python.transactions/test]
    async def test_savepoints_reserve_parents_and_recover_database_errors(self):
        async with self.pool.connection() as connection:
            async with connection.transaction() as tx:
                await tx.execute(self.insert(1))
                child = await tx.begin()
                for action in (tx.commit, tx.rollback, tx.begin, lambda: tx.fetch_all(self.rows())):
                    with self.assertRaises(p.LifecycleError):
                        await action()
                await child.execute(self.insert(2))
                with self.assertRaises(p.DatabaseError) as caught:
                    await child.execute(self.insert(1))
                self.assertEqual(caught.exception.sqlstate, "23505")
                await child.rollback()
                self.assertFalse(tx.closed)
                async with tx.transaction() as child:
                    await child.execute(self.insert(3))
                    async with child.transaction() as grandchild:
                        await grandchild.execute(self.insert(4))
                self.assertEqual([r["id"] for r in await tx.fetch_all(self.rows())], [1, 3, 4])
            self.assertTrue(await connection.ping())
        self.assertEqual([r["id"] for r in await self.pool.fetch_all(self.rows())], [1, 3, 4])

    # [spec:pgorm:req:python.transactions/test]
    async def test_options_and_foreign_parent_rejection(self):
        async with self.pool.connection() as connection, self.pool.connection() as other:
            for mode, isolation in (("read_only", "repeatable_read"), ("read_write", "serializable"), ("deferrable", None)):
                async with connection.transaction(mode=mode, isolation=isolation) as tx:
                    settings = await tx.fetch_one(p.RawSQL("SELECT current_setting('transaction_isolation') AS isolation, current_setting('transaction_read_only') AS read_only, current_setting('transaction_deferrable') AS deferrable"))
                    self.assertEqual(settings["isolation"], (isolation or "serializable").replace("_", " "))
                    self.assertEqual(settings["read_only"], "off" if mode == "read_write" else "on")
                    self.assertEqual(settings["deferrable"], "on" if mode == "deferrable" else "off")
                    with self.assertRaises(p.ConstructionError):
                        p.Transaction(tx._native, other)
            for kwargs in ({"mode": "raw SQL"}, {"isolation": "serializable"}, {"mode": "deferrable", "isolation": "serializable"}, {"mode": "read_only", "isolation": "made up"}):
                with self.assertRaises(p.ConstructionError):
                    await connection.begin(**kwargs)
            tx = await connection.begin(mode="read_only")
            with self.assertRaises(p.DatabaseError) as caught:
                await tx.execute(self.insert(7))
            self.assertEqual(caught.exception.sqlstate, "25006")
            await tx.rollback()

    # [spec:pgorm:req:python.cancellation/test]
    async def test_cancelled_request_discards_scope_within_two_seconds(self):
        async with self.pool.connection() as connection:
            tx = await connection.begin()
            child = await tx.begin()
            pending = asyncio.create_task(child.fetch_one(p.RawSQL("SELECT 1 AS n FROM pg_sleep(2)")))
            await asyncio.sleep(0.05)
            with self.assertRaises(p.LifecycleError):
                await child.fetch_one(p.select(p.literal(1).as_("n")))
            started = time.monotonic()
            pending.cancel()
            with self.assertRaises(asyncio.CancelledError):
                await pending
            await asyncio.wait_for(connection.close(), 2)
            self.assertLess(time.monotonic() - started, 2)
            self.assertTrue(tx.closed)
            self.assertTrue(child.closed)
            self.assertTrue(connection.closed)
            self.assertTrue(await self.pool.ping())

    # [spec:pgorm:req:python.cancellation/test]
    async def test_cancelled_idle_context_rolls_back_cleanly(self):
        entered = asyncio.Event()
        async def work():
            async with self.pool.transaction() as tx:
                await tx.execute(self.insert(1))
                entered.set()
                await asyncio.Event().wait()
        task = asyncio.create_task(work())
        await entered.wait()
        task.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await asyncio.wait_for(task, 2)
        self.assertEqual(await self.pool.fetch_all(self.rows()), [])

    # [spec:pgorm:req:python.cancellation/test]
    async def test_pool_shutdown_releases_idle_transaction(self):
        connection = await self.pool.acquire()
        tx = await connection.begin()
        await tx.execute(self.insert(1))
        await asyncio.wait_for(self.pool.close(), 2)
        self.assertTrue(tx.closed)
        self.assertTrue(connection.closed)
        with self.assertRaises(p.LifecycleError):
            await tx.commit()

    # [spec:pgorm:req:python.cancellation/test]
    async def test_abandoned_savepoint_rolls_back_and_releases_parent(self):
        async with self.pool.transaction() as tx:
            child = await tx.begin()
            await child.execute(self.insert(1))
            del child
            gc.collect()
            async def released():
                while True:
                    try:
                        return await tx.fetch_all(self.rows())
                    except p.LifecycleError:
                        await asyncio.sleep(0.005)
            self.assertEqual(await asyncio.wait_for(released(), 2), [])
            await tx.execute(self.insert(2))
        self.assertEqual([r["id"] for r in await self.pool.fetch_all(self.rows())], [2])

    # [spec:pgorm:req:python.transactions/test]
    async def test_foreign_event_loop_fails_before_query(self):
        async with self.pool.transaction() as tx:
            async def use_transaction():
                await tx.fetch_all(self.rows())
            with self.assertRaises(p.LifecycleError):
                await asyncio.to_thread(lambda: asyncio.run(use_transaction()))
            await tx.execute(self.insert(1))

    # [spec:pgorm:req:python.cancellation/test]
    async def test_exception_with_background_query_discards_scope(self):
        pending = None
        marker = ValueError("block failed during concurrent query")
        with self.assertRaises(ValueError) as caught:
            async with self.pool.transaction() as tx:
                pending = asyncio.create_task(tx.fetch_one(p.RawSQL("SELECT 1 AS n FROM pg_sleep(2)")))
                await asyncio.sleep(0.05)
                raise marker
        self.assertIs(caught.exception, marker)
        with self.assertRaises(p.LifecycleError):
            await asyncio.wait_for(pending, 2)
        self.assertTrue(tx.closed)
        self.assertTrue(await self.pool.ping())


if __name__ == "__main__":
    unittest.main()
