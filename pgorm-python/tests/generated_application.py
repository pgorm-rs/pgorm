"""Execute concrete generated wrappers from a freshly installed application wheel."""

import copy
import inspect
from pathlib import Path
import unittest
from unittest.mock import patch

import pgorm as p
from pgorm import app
from pgorm._registered.compat import check
from pgorm._registered.entities import Change, Inserted, Updated
import registered_entities as fixture


class GeneratedApplication(unittest.IsolatedAsyncioTestCase):
    asyncSetUp = fixture.RegisteredEntities.asyncSetUp
    asyncTearDown = fixture.RegisteredEntities.asyncTearDown

    # [spec:pgorm:req:python.codegen/test]
    async def test_generated_models_keep_native_states(self):
        original = (
            app.Account.active()
            .set_id(1)
            .set_display_name("Generated")
            .set_note(None)
            .set_mood("busy")
        )
        self.assertIsInstance(original, app.AccountActive)
        self.assertEqual(original.get("id").value.kind, "i32")
        self.assertEqual(
            original.get("mood").value.type_name,
            p.TypeName("Mood", schema="python_entities"),
        )
        with patch(
            "subprocess.run", side_effect=AssertionError("query compiled a program")
        ):
            async with self.pool.connection() as connection:
                model = await original.insert(connection)
                self.assertIsInstance(model, app.AccountModel)
                self.assertEqual(
                    (model.id, model.display_name, model.note, model.mood),
                    (1, "Generated|before", None, "busy"),
                )
                self.assertEqual(model["display name"], model.display_name)
                self.assertEqual(
                    model.into_active().get("id").state, p.ActiveState.Unchanged
                )
                changed = model.with_value("display name", "Updated")
                self.assertEqual(changed.display_name, "Updated")
                updated = (
                    await model.into_active()
                    .set_display_name("Saved")
                    .update(connection)
                )
                self.assertEqual((updated.display_name, updated.version), ("Saved", 2))
                query = app.Account.find().filter(app.Account.col("id") == 1)
                self.assertEqual([row.id for row in await query.all(connection)], [1])
                self.assertIsInstance(await query.one(connection), app.AccountModel)
                self.assertIsInstance(await query.one_opt(connection), app.AccountModel)
                self.assertEqual(query.inspect().params[0].kind, "i32")
                self.assertEqual(await updated.into_active().delete(connection), 1)
                self.assertIsNone(await query.one_opt(connection))

    # [spec:pgorm:req:python.codegen/test]
    # [spec:pgorm:req:python.entities+2/test]
    async def test_generated_views_return_typed_versions(self):
        def active(identity, name):
            return app.Account.active().set_id(identity).set_display_name(name).set_note(None)

        renamed = p.ConflictTarget("id").update("display name")
        async with self.pool.connection() as connection:
            first = await app.Account.insert(active(1, "One")).returning_upsert(connection)
            self.assertIsInstance(first, Inserted)
            self.assertIsInstance(first.model, app.AccountModel)
            change = await app.Account.update(
                first.model.into_active().set_note("noted")
            ).returning_change(connection)
            self.assertIsInstance(change, Change)
            self.assertIsInstance(change.new, app.AccountModel)
            self.assertEqual((change.old.note, change.new.note), (None, "noted"))
            changes = await (
                app.Account.update_many()
                .set("note", "bulk")
                .filter(app.Account.col("id") == 1)
                .returning_changes(connection)
            )
            self.assertEqual([(c.old.note, c.new.note) for c in changes], [("noted", "bulk")])
            rows = await (
                app.Account.insert_many([active(1, "Again"), active(2, "Two")])
                .on_conflict(renamed)
                .returning_upserts(connection)
            )
            match rows:
                case [Updated(change), Inserted(model)]:
                    self.assertIsInstance(change.old, app.AccountModel)
                    self.assertEqual((change.old.display_name, change.new.display_name), ("One", "Again"))
                    self.assertEqual(model.display_name, "Two")
                case other:
                    self.fail(f"expected an update then an insert, got {other!r}")

    # [spec:pgorm:req:python.codegen/test]
    async def test_graph_views_keep_optional_models(self):
        async with self.pool.connection() as connection:
            for identity in (1, 2):
                await (
                    app.Account.active()
                    .set_id(identity)
                    .set_display_name("A")
                    .insert(connection)
                )
            await (
                app.Note.active()
                .set_id(10)
                .set_account_id(1)
                .set_body("B")
                .insert(connection)
            )
            query = app.AccountNotes.find(aliases=['notes "東京"'])
            rows = await query.order_by(query.col(0, "id").asc()).all(connection)
            self.assertEqual(
                [(a.id, None if n is None else n.id) for a, n in rows],
                [(1, 10), (2, None)],
            )
            self.assertIsInstance(rows[0][0], app.AccountModel)
            self.assertIsInstance(rows[0][1], app.NoteModel)
            page = await query.cursor("id").after_with(1, 10).first(1).all(connection)
            self.assertEqual([(a.id, n) for a, n in page], [(2, None)])
            self.assertIsInstance(
                await app.AccountOnly.find().one_opt(connection), app.AccountModel
            )
            required = await app.RequiredNotes.find().all(connection)
            self.assertEqual([(a.id, n.id) for a, n in required], [(1, 10)])

    # [spec:pgorm:req:python.codegen/test]
    async def test_views_reject_foreign_entities(self):
        with self.assertRaises(p.LifecycleError):
            app.AccountActive(p.entity("app.Note").active())
        with self.assertRaises(p.ConstructionError):
            app.Account.active().set_id(p.Value(1, "i64"))
        with self.assertRaises(p.ConstructionError):
            app.Account.active().set_mood("unknown")
        async with self.pool.connection() as connection:
            model = (
                await app.Note.active()
                .set_id(1)
                .set_account_id(1)
                .set_body("B")
                .insert(connection)
            )
            with self.assertRaises(p.LifecycleError):
                app.AccountModel(model.native)


class GeneratedContract(unittest.TestCase):
    # [spec:pgorm:req:python.codegen/test]
    def test_compatibility_fails_before_queries(self):
        for key in ("package_version", "pgorm_version", "registry_abi", "features"):
            expected = copy.deepcopy(app._COMPATIBILITY)
            expected[key] = "mismatch"
            with self.subTest(key=key), self.assertRaises(p.UnsupportedCapabilityError):
                check(expected)
        for family in ("entities", "graphs"):
            expected = copy.deepcopy(app._COMPATIBILITY)
            expected[family][0]["table" if family == "entities" else "sources"] = (
                "changed"
            )
            with (
                self.subTest(family=family),
                self.assertRaises(p.UnsupportedCapabilityError),
            ):
                check(expected)
        code = inspect.getsource(app)
        code = code.replace(
            "_check(_COMPATIBILITY)",
            "_COMPATIBILITY['registry_abi'] = -1\n_check(_COMPATIBILITY)",
        )
        with self.assertRaises(p.UnsupportedCapabilityError):
            exec(compile(code, "incompatible.py", "exec"), {})

    # [spec:pgorm:req:python.codegen/test]
    def test_generated_types_follow_compiled_metadata(self):
        stub = Path(app.__file__).with_suffix(".pyi").read_text()
        self.assertIn("def id(self) -> int:", stub)
        self.assertIn("def display_name(self) -> str:", stub)
        self.assertIn("def note(self) -> str | None:", stub)
        self.assertIn(
            "AccountNotesRow: TypeAlias = tuple[AccountModel, NoteModel | None]", stub
        )
        self.assertIn(
            "RequiredNotesRow: TypeAlias = tuple[AccountModel, NoteModel]", stub
        )
        self.assertIn("AccountOnlyRow: TypeAlias = AccountModel", stub)
        self.assertEqual(app.Account.col("id").describe()["rust_decode_type"], "i32")
        self.assertEqual(
            app.Account.col("display name").describe()["json_key"], "displayName"
        )
        self.assertEqual(app.Account.describe()["schema"], "python_entities")
