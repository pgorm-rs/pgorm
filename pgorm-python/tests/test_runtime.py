"""Native PostgreSQL resource tests; require PGORM_TEST_DSN for a disposable DB."""

import asyncio
import os
import unittest
from urllib.parse import urlsplit, urlunsplit, quote

import pgorm


class RuntimeTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.dsn = os.environ["PGORM_TEST_DSN"]
        self.pool = pgorm.Pool(self.dsn, max_size=1, acquire_timeout=0.2)

    async def asyncTearDown(self):
        await self.pool.close()

    # [spec:pgorm:req:python.runtime/test]
    # [spec:pgorm:req:python.connections/test]
    async def test_connection_reuse_and_context_cleanup(self):
        self.assertTrue(await self.pool.ping())
        self.assertEqual(self.pool.status().size, 1)
        async with self.pool.connection() as connection:
            self.assertTrue(await connection.ping())
            self.assertEqual(self.pool.status().available, 0)
        self.assertTrue(connection.closed)
        self.assertEqual(self.pool.status().available, 1)
        self.assertTrue(await self.pool.ping())
        self.assertEqual(self.pool.status().size, 1)

    # [spec:pgorm:req:python.runtime/test]
    async def test_acquisition_does_not_block_asyncio(self):
        async with self.pool.connection():
            waiting = asyncio.create_task(self.pool.acquire())
            ticks = 0
            for _ in range(5):
                await asyncio.sleep(0.01)
                ticks += 1
            self.assertEqual(ticks, 5)
            self.assertFalse(waiting.done())
            waiting.cancel()
            with self.assertRaises(asyncio.CancelledError):
                await waiting
        self.assertTrue(await self.pool.ping())

    # [spec:pgorm:req:python.connections/test]
    async def test_shutdown_releases_checked_out_resources_and_waiters(self):
        connection = await self.pool.acquire()
        waiting = asyncio.create_task(self.pool.acquire())
        await asyncio.sleep(0.02)
        await asyncio.wait_for(self.pool.close(), 1)
        with self.assertRaises(pgorm.LifecycleError):
            await waiting
        self.assertTrue(connection.closed)
        with self.assertRaises(pgorm.LifecycleError):
            await connection.ping()
        with self.assertRaises(pgorm.LifecycleError):
            await self.pool.acquire()
        await self.pool.close()
        await connection.close()

    # [spec:pgorm:req:python.runtime/test]
    async def test_another_event_loop_is_rejected_without_deadlock(self):
        def foreign():
            return asyncio.run(self.pool.ping())
        with self.assertRaises(pgorm.LifecycleError):
            await asyncio.wait_for(asyncio.to_thread(foreign), 1)
        self.assertTrue(await self.pool.ping())

    # [spec:pgorm:req:python.errors/test]
    async def test_acquisition_timeout_is_distinct_from_cancellation(self):
        async with self.pool.connection():
            with self.assertRaises(pgorm.TimeoutError):
                await self.pool.acquire()
        self.assertTrue(await self.pool.ping())
        self.assertIs(pgorm.CancelledError, asyncio.CancelledError)

    # [spec:pgorm:req:python.errors/test]
    async def test_bad_configuration_is_an_explicit_construction_error(self):
        for options in (
            {"max_size": 0}, {"max_size": -1}, {"max_size": True},
            {"max_size": 2**100}, {"statement_cache_size": -1},
            {"acquire_timeout": 0}, {"connect_timeout": float("nan")},
            {"tls": "ignore-certificate"}, {"recycle": "invented"},
        ):
            with self.subTest(options=options):
                with self.assertRaises(pgorm.ConstructionError):
                    pgorm.Pool(self.dsn, **options)
        secret = "do-not-show-this-password"
        with self.assertRaises(pgorm.ConstructionError) as caught:
            pgorm.Pool(f"unknown-key={secret}")
        self.assertNotIn(secret, str(caught.exception))

    # [spec:pgorm:req:python.errors/test]
    # [spec:pgorm:req:python.connections/test]
    async def test_sqlstate_and_credential_redaction(self):
        parts = urlsplit(self.dsn)
        password = "deliberately-invalid-private-password"
        netloc = f"{parts.username}:{quote(password)}@{parts.hostname}:{parts.port}"
        dsn = urlunsplit((parts.scheme, netloc, parts.path, parts.query, parts.fragment))
        pool = pgorm.Pool(dsn)
        try:
            self.assertNotIn(password, repr(pool))
            self.assertNotIn(password, repr(pool._native))
            with self.assertRaises(pgorm.PgOrmError) as caught:
                await pool.ping()
            self.assertEqual(caught.exception.sqlstate, "28P01")
            self.assertNotIn(password, repr(caught.exception))
            self.assertNotIn(parts.username, caught.exception.message)
        finally:
            await pool.close()

    # [spec:pgorm:req:python.connections/test]
    async def test_tls_never_falls_back_to_plaintext(self):
        pool = pgorm.Pool(self.dsn, tls="verify-full")
        try:
            with self.assertRaises(pgorm.PgOrmError):
                await pool.ping()
        finally:
            await pool.close()

    # [spec:pgorm:req:python.connections/test]
    async def test_verified_tls_accepts_the_configured_ca(self):
        async with pgorm.Pool(self.dsn, tls="verify-full", cafile=os.environ["PGORM_TEST_CA"]) as pool:
            self.assertTrue(await pool.ping())

    # [spec:pgorm:req:python.connections/test]
    async def test_certificate_hostname_is_verified(self):
        parts = urlsplit(self.dsn)
        netloc = f"{parts.username}:{quote(parts.password)}@wrong.test:{parts.port}"
        query = f"{parts.query}&hostaddr=127.0.0.1"
        dsn = urlunsplit((parts.scheme, netloc, parts.path, query, parts.fragment))
        pool = pgorm.Pool(dsn, tls="verify-full", cafile=os.environ["PGORM_TEST_CA"])
        try:
            with self.assertRaises(pgorm.PgOrmError):
                await pool.ping()
        finally:
            await pool.close()


if __name__ == "__main__":
    unittest.main()
