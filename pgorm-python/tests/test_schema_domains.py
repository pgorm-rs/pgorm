"""CREATE DOMAIN, ALTER DOMAIN and DROP DOMAIN from the installed native wheel."""
import asyncio
import os
import unittest
from uuid import uuid4

import pgorm as p
from pgorm import schema as s

OPERATIONS = (
    "create_domain", "set_domain_default", "drop_domain_default", "set_domain_not_null",
    "drop_domain_not_null", "add_domain_check", "drop_domain_constraint",
    "rename_domain_constraint", "validate_domain_constraint", "rename_domain", "drop_domain",
)


class DomainConstruction(unittest.TestCase):
    # [spec:pgorm:req:python.schema-domains/test]    a create renders its clauses in the native
    # order whatever order they were given in, each alter carries its one action, a drop names
    # every domain given in order, and nothing is bound
    def test_domain_statements_render_in_native_order(self):
        manifest = p.capabilities()["operations"]
        for operation in OPERATIONS:
            self.assertFalse(manifest["schema." + operation]["registration_required"])
            p.require_capability("schema." + operation)
        p.require_capability("domain_value")
        name = p.TypeName('qty "雪"', schema="billing")
        value = p.domain_value()
        create = (
            s.create_domain(name, "integer").check(value > 0, name="positive").not_null()
            .default(1).check(value < 100).inspect()
        )
        self.assertEqual(
            create.sql,
            'CREATE DOMAIN "billing"."qty ""雪""" AS integer DEFAULT 1 NOT NULL '
            'CONSTRAINT "positive" CHECK (VALUE > 0) CHECK (VALUE < 100)',
        )
        self.assertEqual(create.params, [])
        self.assertEqual(
            s.CreateDomain("code", "text").collate("C").null().default(p.literal(1) + 2).inspect().sql,
            'CREATE DOMAIN "code" AS text COLLATE "C" DEFAULT (1 + 2) NULL',
        )
        self.assertEqual(
            s.create_domain("d", "integer").not_null().null().inspect().sql,
            'CREATE DOMAIN "d" AS integer NULL',
        )
        prefix = 'ALTER DOMAIN "billing"."qty ""雪"""'
        for ddl, tail in (
            (s.set_domain_default(name, 5), "SET DEFAULT 5"),
            (s.drop_domain_default(name), "DROP DEFAULT"),
            (s.set_domain_not_null(name), "SET NOT NULL"),
            (s.drop_domain_not_null(name), "DROP NOT NULL"),
            (s.add_domain_check(name, value > 0), "ADD CHECK (VALUE > 0)"),
            (
                s.add_domain_check(name, value < 9, name="small", not_valid=True),
                'ADD CONSTRAINT "small" CHECK (VALUE < 9) NOT VALID',
            ),
            (s.drop_domain_constraint(name, "small"), 'DROP CONSTRAINT "small"'),
            (
                s.drop_domain_constraint(name, "small", if_exists=True, cascade=True),
                'DROP CONSTRAINT IF EXISTS "small" CASCADE',
            ),
            (s.rename_domain_constraint(name, "small", "tiny"), 'RENAME CONSTRAINT "small" TO "tiny"'),
            (s.validate_domain_constraint(name, "small"), 'VALIDATE CONSTRAINT "small"'),
            (s.rename_domain(name, "units"), 'RENAME TO "units"'),
        ):
            with self.subTest(tail=tail):
                self.assertEqual(ddl.inspect().sql, f"{prefix} {tail}")
                self.assertEqual(ddl.inspect().params, [])
        self.assertEqual(
            s.drop_domain("a", name, p.Identifier("B"), if_exists=True, cascade=True).inspect().sql,
            'DROP DOMAIN IF EXISTS "a", "billing"."qty ""雪""", "B" CASCADE',
        )
        self.assertEqual(s.drop_domain("a").inspect().sql, 'DROP DOMAIN "a"')

    # [spec:pgorm:req:python.schema-domains/test]    a name that is not an identifier, a check
    # that is not an expression and an unknown base type are refused as the statement is built
    def test_invalid_domain_input_is_refused(self):
        for action in (
            lambda: s.create_domain("", "integer"),
            lambda: s.create_domain("x" * 64, "integer"),
            lambda: s.create_domain(p.Table("t"), "integer"),
            lambda: s.create_domain("d", "integer").check("VALUE > 0"),
            lambda: s.create_domain("d", "integer").check(p.domain_value() > 0, name=""),
            lambda: s.create_domain("d", "text").collate(""),
            lambda: s.create_domain("d", "text").collate("C", schema="nul\0"),
            lambda: s.add_domain_check("d", "VALUE > 0"),
            lambda: s.drop_domain_constraint("d", ""),
            lambda: s.rename_domain_constraint("d", "a", ""),
            lambda: s.validate_domain_constraint("", "a"),
            lambda: s.rename_domain("d", ""),
            lambda: s.drop_domain("d", 3),
        ):
            with self.subTest(action=action), self.assertRaises(p.ConstructionError):
                action()
        with self.assertRaises(p.UnsupportedCapabilityError):
            s.create_domain("d", "serial")
        with self.assertRaises(TypeError):
            s.drop_domain()
        with self.assertRaises(TypeError):
            s.rename_domain("a", "b", if_exists=True)
        with self.assertRaises(p.ConstructionError):
            bool(s.create_domain("d", "integer"))
        with self.assertRaises(p.ConstructionError):
            bool(s.drop_domain("d"))

    # [spec:pgorm:req:python.schema-domains/test]    `domain_value` is the keyword `VALUE`
    def test_domain_value_is_the_keyword(self):
        self.assertEqual(p.domain_value().inspect().sql, "SELECT VALUE")


class DomainDatabase(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
        self.namespace = 'domains "雪 ' + uuid4().hex[:8]
        await self.pool.execute(s.create_schema(self.namespace))
        self.domain = p.TypeName('qty "x"', schema=self.namespace)
        self.table = p.Table("line", schema=self.namespace)

    async def asyncTearDown(self):
        await self.pool.execute(s.drop_schema(self.namespace, cascade=True))
        await self.pool.close()

    async def refused(self, statement, sqlstate):
        with self.assertRaises(p.DatabaseError) as refused:
            await self.pool.execute(statement)
        self.assertEqual(refused.exception.sqlstate, sqlstate)

    async def quantities(self):
        rows = await self.pool.fetch_all(
            p.select(p.col("id"), p.col("qty")).from_(self.table).order_by(p.col("id").asc())
        )
        return [(row["id"], row["qty"]) for row in rows]

    async def create(self):
        value = p.domain_value()
        await self.pool.execute(
            s.create_domain(self.domain, "integer").default(7).not_null()
            .check(value > 0, name="positive")
        )
        await self.pool.execute(
            s.create_table(self.table).column(s.ColumnDef("id", "integer").not_null())
            .column(s.ColumnDef("qty", self.domain)).primary_key("id")
        )

    # [spec:pgorm:req:python.schema-domains/test]    a column of the domain takes its default
    # and is held to its check and its NOT NULL, each alter changes what it holds the column
    # to, and a NOT VALID check leaves the rows already there until VALIDATE refuses them
    async def test_domain_holds_its_column_to_its_constraints(self):
        await self.create()
        await self.pool.execute(p.insert(self.table).columns("id").values(1))
        await self.pool.execute(p.insert(self.table).columns("id", "qty").values(2, 3))
        self.assertEqual(await self.quantities(), [(1, 7), (2, 3)])
        await self.refused(p.insert(self.table).columns("id", "qty").values(3, -1), "23514")
        await self.refused(p.insert(self.table).columns("id", "qty").values(3, p.Value.null("i32")), "23502")

        # PostgreSQL's plan cache does not track a domain's default, so an
        # insert the connection prepared before ALTER DOMAIN keeps the default
        # it was planned with; each insert after one is a statement of its own.
        await self.pool.execute(s.set_domain_default(self.domain, 9))
        await self.pool.execute(p.insert(self.table).columns("id").values(3).returning(p.col("qty")))
        await self.pool.execute(s.drop_domain_default(self.domain))
        await self.pool.execute(s.drop_domain_not_null(self.domain))
        await self.pool.execute(p.insert(self.table).columns("id").values(4).returning(p.col("id")))
        self.assertEqual(await self.quantities(), [(1, 7), (2, 3), (3, 9), (4, None)])
        await self.refused(s.set_domain_not_null(self.domain), "23502")
        await self.pool.execute(p.delete(self.table).where_(p.col("id") == 4))
        await self.pool.execute(s.set_domain_not_null(self.domain))

        value = p.domain_value()
        await self.refused(s.add_domain_check(self.domain, value < 5, name="small"), "23514")
        await self.pool.execute(
            s.add_domain_check(self.domain, value < 5, name="small", not_valid=True)
        )
        await self.refused(p.insert(self.table).columns("id", "qty").values(5, 6), "23514")
        await self.refused(s.validate_domain_constraint(self.domain, "small"), "23514")
        await self.pool.execute(p.delete(self.table).where_(p.col("qty") >= 5))
        await self.pool.execute(s.validate_domain_constraint(self.domain, "small"))
        self.assertEqual(await self.quantities(), [(2, 3)])

        await self.pool.execute(s.rename_domain_constraint(self.domain, "small", 'tiny "雪"'))
        await self.pool.execute(s.drop_domain_constraint(self.domain, 'tiny "雪"'))
        await self.pool.execute(s.drop_domain_constraint(self.domain, 'tiny "雪"', if_exists=True))
        await self.pool.execute(p.insert(self.table).columns("id", "qty").values(6, 60))
        self.assertEqual(await self.quantities(), [(2, 3), (6, 60)])

    # [spec:pgorm:req:python.connections+3/test]    clear_statement_caches makes the connection
    # that already ran an insert prepare it again, its old statement closed on the server, and
    # write the domain's new default
    # [spec:pgorm:req:conn.pool.statement-cache.clear+1/test]    from Python
    async def test_a_cleared_pool_writes_the_new_default(self):
        await self.create()

        def insert(row):
            return p.insert(self.table).columns("id").values(row).returning(p.col("qty"))

        prepared = p.RawSQL(
            "SELECT count(*)::integer AS n FROM pg_prepared_statements WHERE statement LIKE $1",
            ["INSERT INTO%"],
        )
        self.assertEqual((await self.pool.fetch_one(insert(1)))["qty"], 7)
        await self.pool.execute(s.set_domain_default(self.domain, 9))
        self.assertEqual((await self.pool.fetch_one(insert(2)))["qty"], 7)
        self.assertEqual((await self.pool.fetch_one(prepared))["n"], 1)

        self.pool.clear_statement_caches()
        self.assertEqual((await self.pool.fetch_one(prepared))["n"], 0)
        self.assertEqual((await self.pool.fetch_one(insert(3)))["qty"], 9)

    async def eventually(self, what, probe):
        deadline = asyncio.get_running_loop().time() + 10
        while not await probe():
            self.assertLess(asyncio.get_running_loop().time(), deadline, what)
            await asyncio.sleep(0.01)

    # [spec:pgorm:req:python.connections+3/test]    a pool given statement_cache_channel listens
    # from its first acquisition, clears on a notification there whatever its payload, and
    # closes its listening connection when it closes; a pool without the setting keeps its
    # statements
    # [spec:pgorm:req:conn.pool.statement-cache.listen+1/test]    from Python
    async def test_a_notified_pool_writes_the_new_default(self):
        p.require_capability("pool.statement_cache_channel")
        self.assertEqual(p.STATEMENT_CACHE_CHANNEL, "pgorm_statement_caches")
        await self.create()
        channel = 'caches "雪 ' + uuid4().hex[:8]
        listening = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1, statement_cache_channel=channel)

        def insert(row):
            return p.insert(self.table).columns("id").values(row).returning(p.col("qty"))

        prepared = p.RawSQL(
            "SELECT count(*)::integer AS n FROM pg_prepared_statements WHERE statement LIKE $1",
            ["INSERT INTO%"],
        )
        listeners = p.RawSQL(
            "SELECT count(*)::integer AS n FROM pg_stat_activity "
            "WHERE datname = current_database() AND query = $1 AND state = 'idle'",
            ['LISTEN "' + channel.replace('"', '""') + '"'],
        )

        async def listening_count():
            return (await self.pool.fetch_one(listeners))["n"]

        try:
            self.assertTrue(await listening.ping())
            await self.eventually("the listener's LISTEN", lambda: self.is_count(listening_count, 1))
            self.assertEqual((await listening.fetch_one(insert(1)))["qty"], 7)
            self.assertEqual((await listening.fetch_one(prepared))["n"], 1)
            self.assertEqual((await self.pool.fetch_one(insert(2)))["qty"], 7)

            await self.pool.execute(s.set_domain_default(self.domain, 9))
            async with self.pool.connection() as connection:
                await connection.notify(channel, "not read")
            await self.eventually(
                "the clear",
                lambda: self.is_count(lambda: self.count(listening, prepared), 0),
            )
            self.assertEqual((await listening.fetch_one(insert(3)))["qty"], 9)
            self.assertEqual((await self.pool.fetch_one(insert(4)))["qty"], 7)

            await listening.close()
            await self.eventually("the listener's close", lambda: self.is_count(listening_count, 0))
        finally:
            await listening.close()

    @staticmethod
    async def count(pool, query):
        return (await pool.fetch_one(query))["n"]

    @staticmethod
    async def is_count(probe, expected):
        return await probe() == expected

    # [spec:pgorm:req:python.schema-domains/test]    a rename keeps the column typed with the
    # domain, a drop is refused under RESTRICT while a column uses it, and CASCADE drops the
    # column with it
    async def test_domain_renames_and_drops(self):
        await self.create()
        await self.pool.execute(s.rename_domain(self.domain, 'units "雪"'))
        renamed = p.TypeName('units "雪"', schema=self.namespace)
        typed = await self.pool.fetch_one(p.RawSQL(
            "SELECT format_type(atttypid, NULL) AS kind FROM pg_attribute "
            "WHERE attrelid = $1::text::regclass AND attname = 'qty'",
            ['"' + self.namespace.replace('"', '""') + '".line'],
        ))
        self.assertTrue(typed["kind"].endswith('"units ""雪"""'), typed["kind"])
        await self.refused(s.drop_domain(renamed), "2BP01")
        await self.pool.execute(s.drop_domain(renamed, cascade=True))
        columns = await self.pool.fetch_all(p.RawSQL(
            "SELECT column_name AS name FROM information_schema.columns "
            "WHERE table_schema = $1 AND table_name = 'line'",
            [self.namespace],
        ))
        self.assertEqual([row["name"] for row in columns], ["id"])

    # [spec:pgorm:req:python.schema-domains/test]    the server's refusals arrive by SQLSTATE: a
    # name a type already has, a missing domain or constraint, and a COLLATE on a type without
    # one
    async def test_server_refusals_by_sqlstate(self):
        await self.create()
        await self.refused(s.create_domain(self.domain, "text"), "42710")
        await self.refused(s.add_domain_check(self.domain, p.domain_value() > 1, name="positive"), "42710")
        missing = p.TypeName("missing", schema=self.namespace)
        await self.refused(s.set_domain_default(missing, 1), "42704")
        await self.refused(s.drop_domain(missing), "42704")
        await self.pool.execute(s.drop_domain(missing, if_exists=True))
        await self.refused(s.drop_domain_constraint(self.domain, "absent"), "42704")
        await self.refused(s.validate_domain_constraint(self.domain, "absent"), "42704")
        await self.refused(
            s.create_domain(p.TypeName("collated", schema=self.namespace), "integer").collate("C"),
            "42804",
        )


if __name__ == "__main__":
    unittest.main()
