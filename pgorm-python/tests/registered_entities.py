"""Run only against the independent application-binding wheel."""

import asyncio
import os
import unittest

import pgorm as p
from pgorm import schema as s


class RegisteredEntities(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
        self.created = False
        await self.pool.execute(p.RawSQL('CREATE SCHEMA python_entities'))
        self.created = True
        await self.pool.execute(p.RawSQL('CREATE TYPE python_entities."Mood" AS ENUM (\'calm\', \'busy\')'))
        await self.pool.execute(p.RawSQL('CREATE TABLE python_entities.accounts (id integer PRIMARY KEY, "display name" text NOT NULL, note text, version integer NOT NULL, mood python_entities."Mood" NOT NULL)'))
        await self.pool.execute(p.RawSQL('CREATE TABLE python_entities.notes (id integer PRIMARY KEY, account_id integer NOT NULL, body text NOT NULL)'))
        await self.pool.execute(p.RawSQL('CREATE TYPE python_entities.floatrange AS RANGE (SUBTYPE = float8)'))
        await self.pool.execute(p.RawSQL('CREATE TABLE python_entities.bookings (id integer PRIMARY KEY, span python_entities.floatrange NOT NULL, spans python_entities.floatmultirange)'))
        self.account = p.entity("app.Account")
        self.note = p.entity("app.Note")

    async def asyncTearDown(self):
        if self.created:
            await self.pool.execute(p.RawSQL('DROP SCHEMA python_entities CASCADE'))
        await self.pool.close()

    def active(self, identity, name="Nora"):
        return self.account.active().set("id", identity).set("display name", name)

    # [spec:pgorm:req:python.entities+2/test]
    async def test_registration_declares_real_types_and_mapping(self):
        registrations = p.capabilities()["registrations"]["entities"]
        self.assertEqual([r["name"] for r in registrations],
                         ["app.Account", "app.Booking", "app.Membership", "app.Note", "app.Old", "app.Room",
                          "app.Stay"])
        info = self.account.describe()
        self.assertTrue(info["rust_entity"].endswith("account::Entity"))
        self.assertTrue(info["rust_model"].endswith("account::Model"))
        self.assertTrue(info["rust_active_model"].endswith("account::ActiveModel"))
        self.assertEqual((info["schema"], info["table"]), ("python_entities", "accounts"))
        self.assertEqual(self.account.col("display name").describe()["json_key"], "displayName")
        self.assertTrue(self.account.col("id").describe()["primary_key"])
        self.assertTrue(self.account.col("note").describe()["nullable"])
        with self.assertRaises(p.UnsupportedCapabilityError):
            p.entity("invented.Entity")
        with self.assertRaises(p.ConstructionError):
            self.account.col("missing")

    # [spec:pgorm:req:python.entities+2/test]
    async def test_active_values_and_hooks_use_rust_states(self):
        initial = self.account.active()
        self.assertEqual(initial.get("id").state, p.ActiveState.NotSet)
        self.assertEqual(initial.get("version").state, p.ActiveState.Set)
        self.assertEqual(initial.get("version").value.value, 1)
        active = self.active(1).set("note", None).set("mood", "busy")
        self.assertEqual(initial.get("id").state, p.ActiveState.NotSet)
        self.assertEqual(active.get("note").state, p.ActiveState.Set)
        self.assertTrue(active.get("note").value.is_null)
        self.assertEqual(active.get("mood").value.type_name, p.TypeName("Mood", schema="python_entities"))
        async with self.pool.connection() as connection:
            model = await active.insert(connection)
            self.assertEqual(dict(model), {"id": 1, "display name": "Nora|before", "note": None, "version": 1, "mood": "busy"})
            loaded = model.into_active()
            self.assertEqual(loaded.get("id").state, p.ActiveState.Unchanged)
            self.assertEqual(loaded.get("note").state, p.ActiveState.Unchanged)
            changed = loaded.set("display name", "Updated").not_set("note")
            updated = await changed.update(connection)
            self.assertEqual(updated["version"], 2)
            self.assertEqual(updated["display name"], "Updated")
            self.assertEqual(model["display name"], "Nora|before")
            self.assertEqual(loaded.reset("id").get("id").state, p.ActiveState.Set)
            self.assertEqual(loaded.get("id").state, p.ActiveState.Unchanged)
            self.assertEqual(await updated.into_active().delete(connection), 1)
            self.assertIsNone(await self.account.find().one_opt(connection))
        self.assertEqual(model.tagged("mood").type_name, p.TypeName("Mood", schema="python_entities"))

    # [spec:pgorm:req:python.entities+2/test]
    # [spec:pgorm:req:python.values+2/test]
    async def test_created_ranges_round_trip_through_an_entity(self):
        booking = p.entity("app.Booking")
        span = p.CreatedRange("floatrange", "f64", schema="python_entities")
        spans = p.CreatedMultirange("floatmultirange", "f64", schema="python_entities")
        self.assertEqual(booking.col("span").describe()["input_hint"],
                         {"kind": "created_range", "name": "floatrange", "schema": "python_entities", "subtype": "f64"})
        self.assertEqual(booking.col("spans").describe()["input_hint"]["kind"], "created_multirange")
        async with self.pool.connection() as connection:
            active = (booking.active().set("id", 1).set("span", p.Range(1.5, 2.5))
                      .set("spans", p.Multirange([p.Range(5.0, 8.0), p.Range(1.0, 3.0)])))
            self.assertEqual(active.get("span").value.created_type, span)
            await active.insert(connection)
            found = await booking.find().filter(booking.col("span") == p.Range(1.5, 2.5)).one(connection)
            self.assertEqual(found["span"], p.Range(1.5, 2.5))
            # The server stores a multirange sorted and merged.
            self.assertEqual(found["spans"], p.Multirange([p.Range(1.0, 3.0), p.Range(5.0, 8.0)]))
            self.assertEqual(found.tagged("span").created_type, span)
            self.assertEqual(found.tagged("spans").created_type, spans)
            row = await connection.fetch_one(p.RawSQL("SELECT span FROM python_entities.bookings"))
            self.assertEqual(row["span"], p.Range(1.5, 2.5))
            self.assertEqual(row.tagged("span"), found.tagged("span"))
            moved = await found.into_active().set("span", p.Range(None, 0.5, "(]")).set("spans", None).update(connection)
            self.assertEqual((moved["span"], moved["spans"]), (p.Range(None, 0.5, "(]"), None))
            with self.assertRaises(p.ConstructionError):
                booking.active().set("span", p.Value(p.Range(1.0, 2.0), p.CreatedRange("floatrange", "f64")))
            with self.assertRaises(p.ConstructionError):
                booking.active().set("span", p.Range(1, 2))

    # [spec:pgorm:req:python.entities+2/test]
    async def test_typed_select_reuses_real_entity_projection(self):
        async with self.pool.connection() as connection:
            for identity in (1, 2, 3):
                await self.active(identity, f"account{identity}").insert(connection)
            query = self.account.find().filter(self.account.col("id") >= 2)
            query = query.order_by(self.account.col("id").expr().desc())
            self.assertEqual([row["id"] for row in await query.all(connection)], [3, 2])
            self.assertEqual((await query.one(connection))["id"], 3)
            self.assertEqual(len(await query.all(connection)), 2)
            self.assertEqual([row["id"] for row in await query.offset(1).limit(1).all(connection)], [2])
            self.assertEqual((await self.account.find().filter(self.account.col("mood") == "calm").one(connection))["mood"], "calm")
            none = query.filter(self.account.col("id") == 100)
            self.assertEqual(await none.all(connection), [])
            self.assertIsNone(await none.one_opt(connection))
            with self.assertRaises(p.DatabaseError):
                await none.one(connection)
            self.assertNotEqual(query.inspect().sql, query.inspect(terminal="one").sql)
            self.assertEqual(query.inspect().params[0].kind, "i32")

    # [spec:pgorm:req:python.entities+2/test]
    async def test_composite_key_entity_writes_by_whole_key(self):
        membership = p.entity("app.Membership")
        self.assertEqual(membership.describe()["primary_keys"], ["tenant_id", "id"])
        flags = [membership.col(name).describe()["primary_key"] for name in ("tenant_id", "id", "role")]
        self.assertEqual(flags, [True, True, False])
        table = s.from_entity(membership).table
        self.assertIn('"id" integer NOT NULL GENERATED BY DEFAULT AS IDENTITY', table.inspect().sql)
        self.assertIn('PRIMARY KEY ("tenant_id", "id")', table.inspect().sql)
        async with self.pool.connection() as connection:
            await connection.execute(table)
            key = await connection.fetch_all(p.RawSQL(
                "SELECT a.attname FROM pg_constraint c, unnest(c.conkey) WITH ORDINALITY AS k(attnum, at) "
                "JOIN pg_attribute a ON a.attnum = k.attnum "
                "WHERE c.conrelid = 'python_entities.memberships'::regclass AND c.contype = 'p' "
                "AND a.attrelid = c.conrelid ORDER BY k.at"
            ))
            self.assertEqual([row["attname"] for row in key], ["tenant_id", "id"])
            # The database generates `id`; a second tenant then holds the same `id`,
            # so a write narrowed by one key column would reach both rows.
            first = await membership.active().set("tenant_id", 1).set("role", "owner").insert(connection)
            shared = first["id"]
            twin = await membership.active().set("tenant_id", 2).set("id", shared).set("role", "guest").insert(connection)
            self.assertEqual((twin["tenant_id"], twin["id"]), (2, shared))
            updated = await first.into_active().set("role", "admin").update(connection)
            self.assertEqual(dict(updated), {"tenant_id": 1, "id": shared, "role": "admin"})
            ordered = membership.find().order_by(membership.col("tenant_id").expr().asc())
            self.assertEqual([dict(row) for row in await ordered.all(connection)], [
                {"tenant_id": 1, "id": shared, "role": "admin"},
                {"tenant_id": 2, "id": shared, "role": "guest"},
            ])
            lookup = membership.find().filter((membership.col("tenant_id") == 2) & (membership.col("id") == shared))
            self.assertEqual((await lookup.one(connection))["role"], "guest")
            with self.assertRaisesRegex(p.DatabaseError, "primary key value is not set"):
                await membership.active().set("tenant_id", 2).set("role", "nobody").update(connection)
            self.assertEqual(await twin.into_active().delete(connection), 1)
            self.assertEqual([dict(row) for row in await ordered.all(connection)], [
                {"tenant_id": 1, "id": shared, "role": "admin"},
            ])

    # [spec:pgorm:req:python.entities+2/test]
    async def test_invalid_types_and_foreign_handles_are_rejected(self):
        for column, value in (("id", True), ("id", 2**31), ("id", "1"), ("mood", "unknown"), ("display name", None)):
            with self.subTest(column=column, value=value), self.assertRaises(p.ConstructionError):
                self.active(1).set(column, value)
        with self.assertRaises(p.ConstructionError):
            self.active(1).set("mood", p.Value("calm", p.TypeName("Mood", schema="foreign")))
        for method in (self.active(1).get, self.active(1).not_set, self.active(1).reset):
            with self.assertRaises(p.LifecycleError):
                method(self.note.col("id"))
        with self.assertRaises(p.LifecycleError):
            self.active(1).set(self.note.col("id"), 1)
        with self.assertRaises(p.ConstructionError):
            bool(self.account.find())
        with self.assertRaises(p.ConstructionError):
            bool(self.account.col("id"))
        with self.assertRaises(p.ConstructionError):
            self.account.find().limit(True)

    # [spec:pgorm:req:python.entities+2/test]
    async def test_model_setter_preserves_rust_conversion_checks(self):
        async with self.pool.connection() as connection:
            model = await self.active(1).insert(connection)
        changed = model.with_value(self.account.col("display name"), "Local")
        self.assertEqual(changed["display name"], "Local")
        self.assertEqual(model["display name"], "Nora|before")
        with self.assertRaises(p.ConstructionError):
            model.with_value("id", p.Value(1, "i64"))
        with self.assertRaises(p.LifecycleError):
            model.with_value(self.note.col("id"), 2)
        self.assertEqual(changed.into_active().get("display name").state, p.ActiveState.Unchanged)

    # [spec:pgorm:req:python.entities+2/test]
    async def test_before_and_after_hooks_keep_write_outcomes(self):
        async with self.pool.connection() as connection:
            with self.assertRaises(p.DatabaseError):
                await self.active(1, "reject_before").insert(connection)
            self.assertIsNone(await self.account.find().one_opt(connection))
            with self.assertRaises(p.DatabaseError):
                await self.active(2, "reject_after").insert(connection)
            self.assertEqual((await self.account.find().one(connection))["id"], 2)
            protected = await self.active(99).insert(connection)
            with self.assertRaises(p.DatabaseError):
                await protected.into_active().delete(connection)
            self.assertIsNotNone(await self.account.find().filter(self.account.col("id") == 99).one_opt(connection))
            after = await self.active(98).insert(connection)
            with self.assertRaises(p.DatabaseError):
                await after.into_active().delete(connection)
            self.assertIsNone(await self.account.find().filter(self.account.col("id") == 98).one_opt(connection))

    # [spec:pgorm:req:python.entities+2/test]
    # [spec:pgorm:req:python.errors/test]
    async def test_decode_failure_does_not_become_absence(self):
        async with self.pool.connection() as connection:
            await self.active(1).insert(connection)
            await connection.execute(p.RawSQL('ALTER TABLE python_entities.accounts ALTER COLUMN version TYPE bigint'))
            with self.assertRaises(p.DecodeError):
                await self.account.find().one_opt(connection)

    # [spec:pgorm:req:python.errors/test]
    async def test_parameter_failure_preserves_connection(self):
        async with self.pool.connection() as connection:
            with self.assertRaises(p.ConstructionError):
                await connection.fetch_one(p.RawSQL("SELECT $1::integer AS n", p.Value("text", "text")))
            self.assertTrue(await connection.ping())

    # [spec:pgorm:req:python.entities+2/test]
    async def test_cancellation_interrupts_an_active_rust_hook(self):
        async with self.pool.connection() as connection:
            future = self.active(1, "wait_before").insert(connection)
            await asyncio.sleep(0.05)
            future.cancel()
            with self.assertRaises(asyncio.CancelledError):
                await asyncio.wait_for(future, 1)
            await asyncio.wait_for(connection.close(), 1)
        self.assertEqual(await self.pool.fetch_all(p.RawSQL('SELECT id FROM python_entities.accounts')), [])


if __name__ == "__main__":
    unittest.main()
