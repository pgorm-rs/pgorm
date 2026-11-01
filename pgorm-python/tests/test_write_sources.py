"""INSERT .. SELECT, OVERRIDING, UPDATE .. FROM, DELETE .. USING, writes in a
WITH clause and conflict arbiters against PostgreSQL, through the installed
wheel."""

import os
import unittest
from uuid import uuid4

import pgorm as p


def quoted(name):
    return '"' + name.replace('"', '""') + '"'


class WriteTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
        self.context = self.pool.connection()
        self.connection = await self.context.__aenter__()
        suffix = uuid4().hex[:8]
        self.names = {
            "account": f'Writes "account" {suffix}',
            "staged": f"writes staged {suffix}",
            "ticket": f"writes ticket {suffix}",
        }
        self.email_key = f"account email {suffix}"
        account, staged, ticket = (quoted(name) for name in self.names.values())
        await self.sql(
            f"CREATE TEMP TABLE {account} (id int PRIMARY KEY, name text NOT NULL, "
            f"email text CONSTRAINT {quoted(self.email_key)} UNIQUE, visits int NOT NULL DEFAULT 0, "
            "active bool NOT NULL DEFAULT true)")
        await self.sql(f"CREATE UNIQUE INDEX ON {account} (lower(name)) WHERE active")
        await self.sql(f"CREATE TEMP TABLE {staged} (id int, name text)")
        await self.sql(
            f"CREATE TEMP TABLE {ticket} (id int GENERATED ALWAYS AS IDENTITY PRIMARY KEY, note text NOT NULL)")
        self.account, self.staged, self.ticket = (p.Table(name) for name in self.names.values())

    async def asyncTearDown(self):
        await self.context.__aexit__(None, None, None)
        await self.pool.close()

    async def sql(self, text):
        await self.connection.execute(p.RawSQL(text))

    async def rows(self, query):
        return sorted(tuple(row.values()) for row in await self.connection.query(query))

    async def accounts(self):
        return await self.rows(p.Select(p.col("id"), p.col("name")).from_(self.account))

    async def seed(self, *names):
        insert = p.Insert(self.account).columns("id", "name")
        for key, name in enumerate(names, 1):
            insert = insert.values(key, name)
        await self.connection.execute(insert)


# [spec:pgorm:req:bindings.write-sources/test]
class WriteSourceTests(WriteTests):
    async def test_insert_takes_rows_from_a_select(self):
        await self.connection.execute(p.Insert(self.staged).columns("id", "name").values(7, "Gil").values(8, "Hal"))
        query = p.Insert(self.account).columns("id", "name").select(
            p.Select(p.col("id"), p.col("name")).from_(self.staged))
        self.assertEqual(await self.connection.execute(query), 2)
        self.assertEqual(await self.accounts(), [(7, "Gil"), (8, "Hal")])
        with self.assertRaises(p.ConstructionError):
            p.Insert(self.account).columns("id").select(p.Select(p.col("id"), p.col("name")).from_(self.staged))

    async def test_overriding_writes_an_identity_column(self):
        explicit = p.Insert(self.ticket).columns("id", "note").values(50, "x")
        with self.assertRaises(p.DatabaseError) as caught:
            await self.connection.execute(explicit)
        self.assertEqual(caught.exception.sqlstate, "428C9")
        written = explicit.overriding(p.Overriding.SystemValue).returning(p.col("id"))
        self.assertEqual(await self.rows(written), [(50,)])
        generated = explicit.overriding(p.Overriding.UserValue).returning(p.col("id"))
        self.assertEqual(await self.rows(generated), [(1,)])

    async def test_update_from_and_delete_using(self):
        await self.seed("Alice", "Bob", "Carol")
        await self.connection.execute(
            p.Insert(self.staged).columns("id", "name").values(1, "Alicia").values(3, "Caroline"))
        s, account = p.Table(self.names["staged"], alias="s"), self.account
        renamed = p.Update(account).set("name", s.col("name")).from_(s).where(s.col("id") == account.col("id"))
        self.assertEqual(await self.connection.execute(renamed), 2)
        self.assertEqual(await self.accounts(), [(1, "Alicia"), (2, "Bob"), (3, "Caroline")])
        gone = p.Select(p.col("id")).from_(self.staged).as_("g")
        removed = (p.Delete(account).using(gone).where(gone.col("id") == account.col("id"))
                   .returning(p.ReturningRow.Old.col("name")))
        self.assertEqual(await self.rows(removed), [("Alicia",), ("Caroline",)])
        self.assertEqual(await self.accounts(), [(2, "Bob")])


# [spec:pgorm:req:bindings.write-ctes/test]
class WriteCteTests(WriteTests):
    async def test_a_returning_write_is_a_cte_body(self):
        await self.seed("Alice", "Bob", "Carol")
        gone = p.Delete(self.account).where(p.col("id") >= 2).returning(p.col("id"), p.col("name"))
        moved = (p.Insert(self.staged).columns("id", "name")
                 .select(p.Select(p.col("id"), p.col("name")).from_(p.Table("gone")))
                 .with_(p.With("gone", gone)))
        self.assertEqual(await self.connection.execute(moved), 2)
        self.assertEqual(await self.accounts(), [(1, "Alice")])
        touched = p.Update(self.staged).set("name", "x").all_rows().returning(p.col("id"))
        counted = p.Select(p.call("count", p.col("id")).as_("n")).from_(p.Table("touched"))
        self.assertEqual(await self.rows(counted.with_(p.With("touched", touched))), [(2,)])
        added = p.Insert(self.staged).columns("id", "name").values(9, "Ida").returning()
        self.assertEqual(await self.rows(p.Select().from_(p.Table("added")).with_(p.With("added", added))),
                         [(9, "Ida")])

    async def test_update_and_delete_take_a_with_clause(self):
        await self.seed("Alice", "Bob")
        await self.connection.execute(p.Insert(self.staged).columns("id", "name").values(1, "a").values(2, "b"))
        w = p.Table("w")
        visited = (p.Update(self.account).set("visits", 5).from_(w).where(w.col("id") == self.account.col("id"))
                   .with_(p.With("w", p.Select(p.literal(1).as_("id")))))
        self.assertEqual(await self.connection.execute(visited), 1)
        dropped = (p.Delete(self.staged).using(w).where(w.col("id") == self.staged.col("id"))
                   .with_(p.With("w", p.Select(p.literal(2).as_("id")))))
        self.assertEqual(await self.connection.execute(dropped), 1)
        self.assertEqual(await self.rows(p.Select(p.col("id"), p.col("visits")).from_(self.account)), [(1, 5), (2, 0)])
        self.assertEqual(await self.rows(p.Select(p.col("id")).from_(self.staged)), [(1,)])

    async def test_unrunnable_write_is_refused_as_read(self):
        for write in (p.Insert(self.account), p.Update(self.account).set("name", "x"), p.Delete(self.account),
                      p.Update(self.account).all_rows()):
            with self.subTest(write=write), self.assertRaises(p.ConstructionError):
                p.With("w", write.returning())


# [spec:pgorm:req:bindings.conflict-arbiters/test]
class ConflictArbiterTests(WriteTests):
    async def test_a_named_constraint_arbitrates(self):
        await self.connection.execute(p.Insert(self.account).columns("id", "name", "email")
                                      .values(1, "Alice", "a@x").values(2, "Bob", "b@x"))
        table = self.names["account"]
        upsert = (p.Insert(self.account).columns("id", "name", "email").values(3, "Carol", "a@x")
                  .on_conflict(p.Conflict.on_constraint(self.email_key)
                               .set("visits", p.col("visits", table=table) + 10)
                               .where(p.col("name", table=table) != "nobody")))
        self.assertEqual(await self.connection.execute(upsert), 1)
        visits = p.Select(p.col("id"), p.col("visits")).from_(self.account)
        self.assertEqual(await self.rows(visits), [(1, 10), (2, 0)])
        skipped = (p.Insert(self.account).columns("id", "name", "email").values(4, "Dan", "b@x")
                   .on_conflict(p.Conflict.on_constraint(self.email_key).do_nothing()))
        self.assertEqual(await self.connection.execute(skipped), 0)
        with self.assertRaises(p.ConstructionError):
            p.Conflict.on_constraint(self.email_key).where(p.col("active") == True)

    async def test_an_index_expression_with_its_predicate_arbitrates(self):
        await self.seed("Alice")
        lower = p.call("lower", p.col("name"))
        partial = (p.Insert(self.account).columns("id", "name").values(4, "ALICE")
                   .on_conflict(p.Conflict.on(lower).where(p.col("active") == True).do_nothing()))
        self.assertEqual(await self.connection.execute(partial), 0)
        unmatched = (p.Insert(self.account).columns("id", "name").values(5, "ALICE")
                     .on_conflict(p.Conflict.on(lower).do_nothing()))
        with self.assertRaises(p.DatabaseError) as caught:
            await self.connection.execute(unmatched)
        self.assertEqual(caught.exception.sqlstate, "42P10")
        with self.assertRaises(TypeError):
            p.Conflict.on(1)


if __name__ == "__main__":
    unittest.main()
