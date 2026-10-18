"""COPY FROM STDIN and COPY TO STDOUT through pgorm's CopyTrait."""

import asyncio
import os
import unittest

import pgorm as p


ITEMS = "python_copy_items"
MIRROR = "python_copy_mirror"


def table_ddl(name):
    return p.RawSQL(
        f'CREATE TABLE "{name}" (id integer PRIMARY KEY, name text NOT NULL, '
        "note text DEFAULT 'none', score integer DEFAULT 7)"
    )


async def collect(stream):
    async with stream:
        return b"".join([chunk async for chunk in stream])


class CopyStatements(unittest.TestCase):
    # [spec:pgorm:req:python.copy+2/test]    the format and direction gate each option
    def test_wrong_format_or_direction_raises_construction_error(self):
        t = p.Table("t")
        refusals = [
            (lambda: p.CopyFrom(t, "text").quote("'"), "csv"),
            (lambda: p.CopyFrom(t, "text").force_null_all(), "csv"),
            (lambda: p.CopyFrom(t, "binary").header(), "binary"),
            (lambda: p.CopyFrom(t, "binary").delimiter(","), "binary"),
            (lambda: p.CopyFrom(t, "binary").on_error_ignore(), "binary"),
            (lambda: p.CopyTo(t, "binary").null_string(""), "binary"),
            (lambda: p.CopyTo(t, "text").force_quote_all(), "csv"),
            (lambda: p.CopyFrom(t, "csv").force_quote_all(), "COPY TO"),
            (lambda: p.CopyTo(t, "csv").freeze(), "COPY FROM"),
            (lambda: p.CopyTo(t, "csv").header_match(), "COPY FROM"),
            (lambda: p.CopyTo(t, "csv").force_null("id"), "COPY FROM"),
            (lambda: p.CopyTo(t, "csv").columns("id"), "Select"),
            (lambda: p.CopyTo(t, "csv").where_(p.col("id") > 1), "Select"),
            (lambda: p.CopyFrom(t, "yaml"), "format"),
            (lambda: p.CopyFrom(t, "csv").delimiter("ab"), "one-character"),
            (lambda: p.CopyFrom(t, "csv").quote(""), "one-character"),
            (lambda: p.CopyFrom(t, "csv").null_string("a\0b"), "NUL"),
            (lambda: p.CopyFrom(t, "csv").encoding(8), "string"),
            (lambda: p.CopyFrom(t, "csv").on_error_ignore(reject_limit=0), "reject_limit"),
            (lambda: p.CopyFrom(t, "csv").on_error_ignore(reject_limit=True), "reject_limit"),
            (lambda: p.CopyFrom(t, "csv").on_error_ignore(log_verbosity="loud"), "log_verbosity"),
            (lambda: p.CopyFrom(t, "csv").columns("id", "id"), "duplicate"),
            (lambda: p.CopyFrom(t.as_("x"), "csv"), "alias"),
            (lambda: p.CopyTo(p.RawSQL("SELECT 1"), "csv"), "Select"),
            (lambda: p.CopyTo(p.update(t).set("id", 1), "csv"), "where_"),
        ]
        for build, reason in refusals:
            with self.subTest(reason=reason), self.assertRaises(p.ConstructionError) as caught:
                build()
            self.assertIn(reason, str(caught.exception))

    # [spec:pgorm:req:python.copy+2/test]    a write without RETURNING is refused as a COPY TO source
    def test_write_without_returning_is_refused(self):
        t, s = p.Table("t"), p.Table("s")
        merge = p.merge(t, s, t.col("id") == s.col("id")).when_matched(p.MatchedAction.Delete)
        writes = [
            (p.insert(t).columns("id").values(1), "INSERT"),
            (p.update(t).set("id", 1).all_rows(), "UPDATE"),
            (p.delete(t).where_(p.col("id") == 1), "DELETE"),
            (merge, "MERGE"),
        ]
        for write, keyword in writes:
            for format in ("text", "csv", "binary"):
                with self.subTest(write=keyword, format=format):
                    with self.assertRaises(p.ConstructionError) as caught:
                        p.CopyTo(write, format)
                    self.assertIn(f"this {keyword} has no RETURNING list", str(caught.exception))
        returning = [
            p.insert(t).columns("id").values(1).returning(p.col("id")),
            p.update(t).set("id", 1).all_rows().returning(p.col("id")),
            p.delete(t).where_(p.col("id") == 1).returning(p.col("id")),
            merge.returning_action(),
        ]
        for write in returning:
            with self.subTest(sql=write.inspect().sql):
                self.assertIn("RETURNING", p.CopyTo(write, "text").inspect().sql)

    # [spec:pgorm:req:python.copy+2/test]    inspection is the Rust rendering, values inline
    def test_inspection_renders_rust_sql_without_parameters(self):
        t = p.Table("glyph", schema="art")
        base = p.CopyFrom(t, "csv")
        built = (
            base.columns("id", "image").header().null_string("").force_null("image")
            .where_(p.col("id") > p.bind(2))
        )
        self.assertEqual(base.inspect().sql, 'COPY "art"."glyph" FROM STDIN WITH (FORMAT csv)')
        self.assertEqual(
            built.inspect().sql,
            'COPY "art"."glyph" ("id", "image") FROM STDIN WITH (FORMAT csv, NULL \'\', '
            'HEADER true, FORCE_NULL ("image")) WHERE "id" > 2',
        )
        self.assertEqual(built.inspect().params, [])
        self.assertEqual((base.format, built.format), ("csv", "csv"))
        ignore = p.CopyFrom(t, "text").on_error_ignore(5, "verbose").inspect().sql
        self.assertIn("ON_ERROR ignore, REJECT_LIMIT 5", ignore)
        self.assertIn("LOG_VERBOSITY verbose", ignore)
        query = p.select(p.col("id")).from_(p.Table("glyph")).where_(p.col("name") == p.bind("it's"))
        out = p.CopyTo(query, "csv").header().force_quote_all().inspect()
        self.assertEqual(
            out.sql,
            'COPY (SELECT "id" FROM "glyph" WHERE "name" = E\'it\\\'s\') TO STDOUT '
            "WITH (FORMAT csv, HEADER true, FORCE_QUOTE *)",
        )
        self.assertEqual(out.params, [])
        self.assertEqual(p.CopyTo(t, "binary").inspect().sql, 'COPY "art"."glyph" TO STDOUT WITH (FORMAT binary)')


class CopyExecution(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=3)
        for name in (ITEMS, MIRROR):
            await self.pool.execute(p.RawSQL(f'DROP TABLE IF EXISTS "{name}"'))
            await self.pool.execute(table_ddl(name))
        self.items = p.Table(ITEMS)
        self.mirror = p.Table(MIRROR)

    async def asyncTearDown(self):
        if self.pool.closed:
            self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=3)
        for name in (ITEMS, MIRROR):
            await self.pool.execute(p.RawSQL(f'DROP TABLE IF EXISTS "{name}"'))
        await asyncio.wait_for(self.pool.close(), 2)

    def rows(self, table=None):
        table = table or self.items
        return p.select(p.col("id"), p.col("name"), p.col("note"), p.col("score")).from_(table).order_by(p.col("id").asc())

    async def stored(self, executor=None, table=None):
        records = await (executor or self.pool).fetch_all(self.rows(table))
        return [tuple(record.values()) for record in records]

    # [spec:pgorm:req:python.copy+2/test]    text and csv data both ways
    async def test_text_and_csv_copy_in_and_out(self):
        text = p.CopyFrom(self.items, "text").columns("id", "name", "note", "score")
        self.assertEqual(await self.pool.copy_in(text, b"1\talpha\t\\N\t1\n2\tbeta\tb\t2\n"), 2)
        csv = p.CopyFrom(self.items, "csv").columns("id", "name", "note").header()
        source = [b"id,name,note\n", bytearray(b'3,"gamma, quoted",\n'), memoryview(b"4,delta,d\n")]
        self.assertEqual(await self.pool.copy_in(csv, source), 2)
        self.assertEqual(await self.stored(), [
            (1, "alpha", None, 1), (2, "beta", "b", 2), (3, "gamma, quoted", None, 7), (4, "delta", "d", 7),
        ])
        out = await collect(await self.pool.copy_out(p.CopyTo(self.items, "text")))
        self.assertEqual(out.splitlines()[:2], [b"1\talpha\t\\N\t1", b"2\tbeta\tb\t2"])
        query = p.select(p.col("id"), p.col("name")).from_(self.items).order_by(p.col("id").asc())
        out = await collect(await self.pool.copy_out(p.CopyTo(query, "csv").header().force_quote("name")))
        self.assertEqual(out, b'id,name\n1,"alpha"\n2,"beta"\n3,"gamma, quoted"\n4,"delta"\n')

    # [spec:pgorm:req:python.copy+2/test]    binary out of one table is binary into another
    async def test_binary_round_trip_between_tables(self):
        await self.pool.copy_in(
            p.CopyFrom(self.items, "csv"),
            (f"{n},name {n},{'' if n % 3 else 'n'},{n * 2}\n".encode() for n in range(1, 2001)),
        )
        async with self.pool.connection() as connection:
            data = await collect(await connection.copy_out(p.CopyTo(self.items, "binary")))
            self.assertTrue(data.startswith(b"PGCOPY\n\xff\r\n\0"))
            self.assertEqual(await connection.copy_in(p.CopyFrom(self.mirror, "binary"), data), 2000)
        self.assertEqual(await self.stored(table=self.mirror), await self.stored())

    # [spec:pgorm:req:python.copy+2/test]    omitted columns take their defaults
    async def test_column_subset_takes_defaults(self):
        statement = p.CopyFrom(self.items, "text").columns("id").columns("name")
        self.assertEqual(await self.pool.copy_in(statement, b"5\tfive\n"), 1)
        self.assertEqual(await self.stored(), [(5, "five", "none", 7)])
        where = p.CopyFrom(self.items, "csv").columns("id", "name").where_(p.col("id") > 6)
        self.assertEqual(await self.pool.copy_in(where, b"6,six\n7,seven\n"), 1)
        self.assertEqual([row[0] for row in await self.stored()], [5, 7])

    # [spec:pgorm:req:python.copy+2/test]    a refused row raises 23505 and aborts the transaction
    async def test_unique_violation_aborts_transaction_until_rollback(self):
        async with self.pool.connection() as connection:
            tx = await connection.begin()
            await tx.execute(p.insert(self.items).columns("id", "name").values(1, "kept"))
            with self.assertRaises(p.DatabaseError) as caught:
                await tx.copy_in(p.CopyFrom(self.items, "text").columns("id", "name"), b"2\tb\n1\tdup\n3\tc\n")
            self.assertEqual(caught.exception.sqlstate, "23505")
            with self.assertRaises(p.DatabaseError) as caught:
                await tx.fetch_all(self.rows())
            self.assertEqual(caught.exception.sqlstate, "25P02")
            await tx.rollback()
            self.assertEqual(await self.stored(connection), [])
            self.assertTrue(await connection.ping())

    # [spec:pgorm:req:python.copy+2/test]    a raising source aborts and the connection stays usable
    async def test_raising_source_aborts_and_keeps_connection(self):
        marker = ValueError("source failed")

        async def failing():
            yield b"".join(f"{n}\tname\n".encode() for n in range(1, 20001))
            raise marker

        def bad_item():
            yield b"1\tone\n"
            yield "2\ttwo\n"

        statement = p.CopyFrom(self.items, "text").columns("id", "name")
        async with self.pool.connection() as connection:
            pid = (await connection.fetch_one(p.RawSQL("SELECT pg_backend_pid() AS pid")))["pid"]
            with self.assertRaises(ValueError) as caught:
                await connection.copy_in(statement, failing())
            self.assertIs(caught.exception, marker)
            self.assertEqual(await self.stored(connection), [])
            with self.assertRaises(p.ConstructionError):
                await connection.copy_in(statement, bad_item())
            for invalid in ("1\tone\n", 7, None):
                with self.assertRaises(p.ConstructionError):
                    await connection.copy_in(statement, invalid)
            with self.assertRaises(p.ConstructionError):
                await connection.execute(statement)
            self.assertEqual(await self.stored(connection), [])
            self.assertEqual(await connection.copy_in(statement, [b"9\tnine\n"]), 1)
            again = (await connection.fetch_one(p.RawSQL("SELECT pg_backend_pid() AS pid")))["pid"]
            self.assertEqual(pid, again)
            self.assertFalse(connection.closed)

    # [spec:pgorm:req:python.copy+2/test]    a raising source inside a transaction aborts it until rollback
    async def test_raising_source_aborts_enclosing_transaction(self):
        async def failing():
            yield b"1\tone\n"
            raise RuntimeError("source failed")

        async with self.pool.connection() as connection:
            tx = await connection.begin()
            with self.assertRaises(RuntimeError):
                await tx.copy_in(p.CopyFrom(self.items, "text").columns("id", "name"), failing())
            self.assertFalse(tx.closed)
            with self.assertRaises(p.DatabaseError) as caught:
                await tx.execute(p.insert(self.items).columns("id", "name").values(2, "two"))
            self.assertEqual(caught.exception.sqlstate, "25P02")
            await tx.rollback()
            self.assertEqual(await self.stored(connection), [])

    # [spec:pgorm:req:python.copy+2/test]    8 MB streamed lazily in, then counted out chunk by chunk
    async def test_large_lazy_source_streams_both_ways(self):
        rows, width = 80_000, 100
        expected = sum(len(f"{n}\t{'x' * width}\n") for n in range(rows))
        self.assertGreater(expected, 8 * 1024 * 1024)
        progress = []

        async def generate():
            for n in range(rows):
                if n == rows * 3 // 4:
                    # The server has consumed data before the source is exhausted.
                    record = await self.pool.fetch_one(p.RawSQL(
                        "SELECT coalesce(max(bytes_processed), 0) AS done FROM pg_stat_progress_copy"
                    ))
                    progress.append(record["done"])
                yield f"{n}\t{'x' * width}\n".encode()

        statement = p.CopyFrom(self.mirror, "text").columns("id", "name")
        self.assertEqual(await self.pool.copy_in(statement, generate()), rows)
        self.assertGreater(progress[0], 0)
        total = chunks = 0
        query = p.select(p.col("id"), p.col("name")).from_(self.mirror).order_by(p.col("id").asc())
        async with await self.pool.copy_out(p.CopyTo(query, "text")) as stream:
            async for chunk in stream:
                self.assertIsInstance(chunk, bytes)
                self.assertLessEqual(len(chunk), 64 * 1024 + width + 16)
                total += len(chunk)
                chunks += 1
        self.assertEqual(total, expected)
        self.assertGreater(chunks, 100)
        self.assertTrue(await self.pool.ping())

    # [spec:pgorm:req:python.copy+2/test]    COPY TO over a query with a WHERE value, and each write's RETURNING
    async def test_copy_to_query_with_where_value(self):
        await self.pool.copy_in(p.CopyFrom(self.items, "csv").columns("id", "name"), b"1,a\n2,b\n3,c\n")
        query = (
            p.select(p.col("id"), p.col("name")).from_(self.items)
            .where_(p.col("name") != p.bind("b")).order_by(p.col("id").desc())
        )
        self.assertEqual(await collect(await self.pool.copy_out(p.CopyTo(query, "csv"))), b"3,c\n1,a\n")
        write = p.insert(self.items).columns("id", "name").values(4, "d").returning(p.col("id"))
        self.assertEqual(await collect(await self.pool.copy_out(p.CopyTo(write, "text"))), b"4\n")
        update = (p.update(self.items).set("score", 0).where_(p.col("id") == 1)
                  .returning(p.col("id"), p.col("score")))
        self.assertEqual(await collect(await self.pool.copy_out(p.CopyTo(update, "csv"))), b"1,0\n")
        delete = p.delete(self.items).where_(p.col("id") == 3).returning(p.col("name"))
        self.assertEqual(await collect(await self.pool.copy_out(p.CopyTo(delete, "text"))), b"c\n")
        merge = (p.merge(self.items, self.mirror, self.items.col("id") == self.mirror.col("id"))
                 .when_not_matched(p.MergeInsert("id", self.mirror.col("id"))
                                   .and_value("name", self.mirror.col("name")))
                 .returning_action().returning(self.items.col("id")))
        await self.pool.execute(p.insert(self.mirror).columns("id", "name").values(5, "e"))
        self.assertEqual(await collect(await self.pool.copy_out(p.CopyTo(merge, "text"))), b"INSERT\t5\n")
        with self.assertRaises(p.ConstructionError):
            p.CopyTo(p.delete(self.items).where_(p.col("id") == 4), "text")
        self.assertEqual([row[0] for row in await self.stored()], [1, 2, 4, 5])

    # [spec:pgorm:req:python.copy+2/test]    a Select source's refused tie limit or window is refused before COPY TO runs
    async def test_copy_to_checks_select_source(self):
        await self.pool.copy_in(p.CopyFrom(self.items, "csv").columns("id", "name", "score"),
                                b"1,a,3\n2,b,3\n3,c,4\n")
        tied = p.select(p.col("id")).from_(self.items).limit_with_ties(1)
        windowed = p.Select(p.call("count", p.col("id")).over("w")).from_(self.items)
        refused = [(tied, "ORDER BY"), (windowed, '"w"')]
        for query, reason in refused:
            for format in ("text", "csv", "binary"):
                with self.subTest(reason=reason, format=format):
                    with self.assertRaises(p.ConstructionError) as caught:
                        p.CopyTo(query, format)
                    self.assertIn(reason, str(caught.exception))
        ordered = tied.order_by(p.col("score").asc())
        out = await collect(await self.pool.copy_out(p.CopyTo(ordered, "text")))
        # Both rows tie on the one key, so their order is the server's.
        self.assertEqual(sorted(out.split(b"\n")), [b"", b"1", b"2"])
        declared = windowed.window("w", p.Window())
        self.assertEqual(await collect(await self.pool.copy_out(p.CopyTo(declared, "csv"))), b"3\n3\n3\n")

    # [spec:pgorm:req:python.copy+2/test]    a copy in progress reserves its connection or transaction
    async def test_active_copy_reserves_connection_and_transaction(self):
        refused = []
        async with self.pool.connection() as connection:
            async def source():
                yield b"1\tone\n"
                for attempt in (connection.ping, lambda: connection.execute(self.rows())):
                    try:
                        await attempt()
                    except p.LifecycleError as error:
                        refused.append(error)
                yield b"2\ttwo\n"

            statement = p.CopyFrom(self.items, "text").columns("id", "name")
            self.assertEqual(await connection.copy_in(statement, source()), 2)
            self.assertEqual(len(refused), 2)
            stream = await connection.copy_out(p.CopyTo(self.items, "csv"))
            with self.assertRaises(p.LifecycleError):
                await connection.fetch_all(self.rows())
            with self.assertRaises(p.LifecycleError):
                await connection.copy_out(p.CopyTo(self.items, "csv"))
            self.assertEqual((await collect(stream)).count(b"\n"), 2)
            self.assertTrue(await connection.ping())
            async with connection.transaction() as tx:
                stream = await tx.copy_out(p.CopyTo(self.items, "text"))
                for attempt in (lambda: tx.fetch_all(self.rows()), tx.commit, tx.begin):
                    with self.assertRaises(p.LifecycleError):
                        await attempt()
                self.assertEqual((await collect(stream)).count(b"\n"), 2)
                self.assertEqual(len(await tx.fetch_all(self.rows())), 2)
                self.assertEqual(await tx.copy_in(statement, b"3\tthree\n"), 1)
            self.assertEqual(len(await connection.fetch_all(self.rows())), 3)

    # [spec:pgorm:req:python.copy+2/test]    a pool copy holds its own connection until the end
    async def test_pool_copy_owns_and_releases_its_connection(self):
        pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1, acquire_timeout=0.5)
        try:
            data = b"".join(f"{n},n\n".encode() for n in range(1, 101))
            self.assertEqual(await pool.copy_in(p.CopyFrom(self.items, "csv").columns("id", "name"), data), 100)
            stream = await pool.copy_out(p.CopyTo(self.items, "csv"))
            with self.assertRaises(p.TimeoutError):
                await pool.acquire()
            self.assertEqual((await collect(stream)).count(b"\n"), 100)
            self.assertTrue(stream.closed)
            self.assertTrue(await pool.ping())
        finally:
            await pool.close()

    # [spec:pgorm:req:python.copy+2/test]    closing COPY TO early discards its connection
    async def test_early_close_discards_connection(self):
        await self.pool.copy_in(
            p.CopyFrom(self.items, "text").columns("id", "name"),
            (f"{n}\t{'y' * 200}\n".encode() for n in range(1, 50001)),
        )
        async with self.pool.connection() as connection:
            async with await connection.copy_out(p.CopyTo(self.items, "text")) as stream:
                self.assertTrue((await anext(stream)).startswith(b"1\t"))
            self.assertTrue(connection.closed)
        async with self.pool.connection() as connection:
            tx = await connection.begin()
            stream = await tx.copy_out(p.CopyTo(self.items, "text"))
            await anext(stream)
            await stream.aclose()
            self.assertTrue(tx.closed)
            self.assertTrue(connection.closed)
        self.assertTrue(await self.pool.ping())

    # [spec:pgorm:req:python.copy+2/test]    a refused COPY FROM raises 42P01 and the pool lends a working connection
    async def test_refused_copy_in_closes_its_connection(self):
        missing = p.Table("python_copy_missing")
        async with self.pool.connection() as connection:
            with self.assertRaises(p.DatabaseError) as caught:
                await connection.copy_in(p.CopyFrom(missing, "text"), b"1\n")
            self.assertEqual(caught.exception.sqlstate, "42P01")
            self.assertTrue(connection.closed)
        with self.assertRaises(p.DatabaseError) as caught:
            await self.pool.copy_in(p.CopyFrom(self.items, "csv").columns("absent"), b"1\n")
        self.assertEqual(caught.exception.sqlstate, "42703")
        async with self.pool.connection() as connection:
            tx = await connection.begin()
            with self.assertRaises(p.DatabaseError) as caught:
                await tx.copy_in(p.CopyFrom(missing, "text"), b"1\n")
            self.assertEqual(caught.exception.sqlstate, "42P01")
            self.assertTrue(tx.closed)
        async with self.pool.connection() as connection:
            with self.assertRaises(p.DatabaseError) as caught:
                await connection.copy_out(p.CopyTo(missing, "text"))
            self.assertEqual(caught.exception.sqlstate, "42P01")
            self.assertFalse(connection.closed)
            self.assertTrue(await connection.ping())
        for _ in range(3):
            self.assertEqual((await self.pool.fetch_one(p.RawSQL("SELECT 1 AS one")))["one"], 1)

    # [spec:pgorm:req:python.copy+2/test]    cancelling a copy during a slow source commits nothing
    # [spec:pgorm:req:python.cancellation/test]
    async def test_cancelled_copy_commits_nothing(self):
        statement = p.CopyFrom(self.items, "text").columns("id", "name")
        for in_transaction in (False, True):
            pulled = asyncio.Event()

            async def slow():
                yield b"".join(f"{n}\tname\n".encode() for n in range(1, 10001))
                pulled.set()
                await asyncio.Event().wait()

            async def work():
                if in_transaction:
                    async with self.pool.transaction() as tx:
                        await tx.copy_in(statement, slow())
                else:
                    await self.pool.copy_in(statement, slow())

            with self.subTest(in_transaction=in_transaction):
                task = asyncio.create_task(work())
                await asyncio.wait_for(pulled.wait(), 5)
                task.cancel()
                with self.assertRaises(asyncio.CancelledError):
                    await asyncio.wait_for(task, 2)
                self.assertEqual(await self.stored(), [])
                self.assertTrue(await asyncio.wait_for(self.pool.ping(), 2))


    # [spec:pgorm:req:python.copy+2/test]    a copy cancelled while finishing discards its connection
    # [spec:pgorm:req:python.cancellation/test]
    async def test_cancel_during_finish_discards_connection(self):
        await self.pool.execute(p.RawSQL(
            "CREATE OR REPLACE FUNCTION python_copy_slow() RETURNS trigger LANGUAGE plpgsql "
            "AS $$ BEGIN PERFORM pg_sleep(1.5); RETURN NULL; END $$"
        ))
        await self.pool.execute(p.RawSQL(
            f'CREATE TRIGGER python_copy_slow AFTER INSERT ON "{MIRROR}" '
            "FOR EACH STATEMENT EXECUTE FUNCTION python_copy_slow()"
        ))
        pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
        try:
            connection = await pool.acquire()
            before = (await connection.fetch_one(p.RawSQL("SELECT pg_backend_pid() AS pid")))["pid"]
            statement = p.CopyFrom(self.mirror, "text").columns("id", "name")
            # The statement trigger sleeps after the last row, inside finish.
            task = asyncio.create_task(connection.copy_in(statement, b"1\tone\n"))
            await asyncio.sleep(0.5)
            self.assertFalse(task.done())
            task.cancel()
            with self.assertRaises(asyncio.CancelledError):
                await asyncio.wait_for(task, 1)

            async def discarded():
                while not connection.closed:
                    await asyncio.sleep(0.02)

            await asyncio.wait_for(discarded(), 5)
            after = (await pool.fetch_one(p.RawSQL("SELECT pg_backend_pid() AS pid")))["pid"]
            self.assertNotEqual(before, after)
        finally:
            await pool.close()


if __name__ == "__main__":
    unittest.main()
