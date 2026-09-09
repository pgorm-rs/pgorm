"""Runtime Python model descriptors executed through installed native builders."""

from dataclasses import FrozenInstanceError
import datetime as dt
from decimal import Decimal
import os
import unittest
from unittest.mock import patch
from uuid import UUID

import pgorm as p


def accounts():
    return p.Model(
        'accounts "log',
        {
            "id": p.Column("i32", primary_key=True),
            "display_name": p.Column("text", name="display name"),
            "note": p.Column("text", nullable=True),
            "mood": p.Column(p.TypeName("Mood", schema="python_models")),
            "data": p.Column("json", nullable=True),
            "tags": p.Column("text", array=True, nullable=True),
        },
        schema="python_models",
    )


class ModelConstruction(unittest.TestCase):
    # [spec:pgorm:req:python.models/test]
    def test_descriptor_names_and_values_are_owned(self):
        from pgorm.models.columns import MODEL_KINDS

        self.assertEqual(
            set(p.capabilities()["model_policy"]["scalar_kinds"]), MODEL_KINDS
        )
        for operation in (
            "model.declare",
            "model.column",
            "model.select",
            "model.insert",
            "model.update",
            "model.delete",
            "model.records",
        ):
            p.require_capability(operation)
        fields = {"identity": p.Column("i32", name='id"雪', primary_key=True)}
        model = p.Model('t"表', fields, schema='s"文')
        fields.clear()
        self.assertEqual(model.primary_keys, ("identity",))
        self.assertIn(
            '"s""文"."t""表"."id""雪" AS "identity"', model.find().inspect().sql
        )
        with self.assertRaises(TypeError):
            model.columns["other"] = p.Column("text")
        with self.assertRaises(FrozenInstanceError):
            model.table = p.Table("other")
        data = {"id": 1, "display_name": "Nora", "mood": "calm", "tags": ["a"]}
        write = accounts().insert(data)
        data["tags"].append("changed")
        self.assertEqual(write.inspect().params[-1].value, ["a"])
        self.assertEqual(write.inspect().params[0].kind, "i32")

    # [spec:pgorm:req:python.models/test]
    def test_unknown_fields_and_collisions_fail_early(self):
        model = accounts()
        for operation in (
            lambda: model.col("missing"),
            lambda: model.select("id", "id"),
            lambda: model.insert({"missing": 1}),
            lambda: model.update({}),
            lambda: model.key({}),
            lambda: model.key({"id": 1, "note": "extra"}),
            lambda: p.Model("t", {}),
            lambda: p.Model(
                "t", {"a": p.Column("i32", name="x"), "b": p.Column("i32", name="x")}
            ),
            lambda: p.Column("i32", nullable=True, primary_key=True),
            lambda: p.Column("i32", nullable=1),
            lambda: p.Column(p.TypeName("Unqualified")),
            lambda: model.as_("a").insert({"id": 1}),
            lambda: p.ModelColumn("id", "i32", p.col("id")),
            lambda: p.ModelQuery(model, p.Insert(model.table), ("id",)),
            lambda: p.ModelRows(model, "SELECT 1", ("id",)),
            lambda: p.ModelWrite(model, p.Select(p.literal(1))),
        ):
            with self.assertRaises(p.ConstructionError):
                operation()
        for kind in ("u64", "char", "datetime_fixed", "datetime_local", "made-up"):
            with (
                self.subTest(kind=kind),
                self.assertRaises(p.UnsupportedCapabilityError),
            ):
                p.Column(kind)
        for value in (True, 2**31, "1", None, p.Value(1, "i64")):
            with self.subTest(value=value), self.assertRaises(p.ConstructionError):
                model.insert({"id": value})
        with self.assertRaises(p.ConstructionError):
            model.insert({"mood": p.Value("calm", p.TypeName("Mood", schema="wrong"))})

    # [spec:pgorm:req:python.models/test]
    def test_queries_keep_native_sql_and_parameter_tags(self):
        model = accounts().as_("a")
        query = (
            model.select("id", "display_name")
            .filter(model.col("id") >= 2)
            .order_by(model.col("id").desc())
            .limit(3)
        )
        native = p.Select(
            model.table.col("id").as_("id"),
            model.table.col("display name").as_("display_name"),
        ).from_(model.table)
        native = (
            native.where_(model.table.col("id") >= p.Value(2, "i32"))
            .order_by(model.table.col("id").desc())
            .limit(3)
        )
        self.assertEqual(query.inspect().sql, native.inspect().sql)
        self.assertEqual(query.inspect().params, native.inspect().params)
        self.assertNotIn("WHERE", model.find().inspect().sql)
        self.assertEqual(
            model.col("id").is_in([1, 2]).inspect().params,
            [p.Value(1, "i32"), p.Value(2, "i32")],
        )
        self.assertIn("IS NULL", model.col("note").is_null().inspect().sql)
        for value in (model.col("id"), query, accounts().delete()):
            with self.assertRaises(p.ConstructionError):
                bool(value)
        with self.assertRaises(p.ConstructionError):
            model.col("id") == model.col("display_name")

    # [spec:pgorm:req:python.models/test]
    def test_crud_preserves_omission_and_write_guards(self):
        model = accounts()
        omitted = model.insert({"id": 1}).inspect()
        explicit = model.insert({"id": 1, "note": None}).inspect()
        self.assertNotIn('"note"', omitted.sql)
        self.assertIn('"note"', explicit.sql)
        self.assertTrue(explicit.params[1].is_null)
        self.assertEqual(
            model.insert({}).inspect().sql,
            p.Insert(model.table).default_values().inspect().sql,
        )
        null_json = model.insert({"data": None}).inspect().params[0]
        json_null = model.insert({"data": p.Value.json(None)}).inspect().params[0]
        self.assertTrue(null_json.is_null)
        self.assertFalse(json_null.is_null)
        for query in (model.update({"note": None}), model.delete()):
            with self.assertRaises(p.ConstructionError):
                query.inspect()
            query.all_rows().inspect()
            query.where_(model.key({"id": 1})).inspect()
        first = model.insert({"id": 1, "display_name": "A"}).inspect()
        second = model.insert({"display_name": "A", "id": 1}).inspect()
        self.assertEqual((first.sql, first.params), (second.sql, second.params))


class ModelDatabase(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
        await self.pool.execute(p.RawSQL("CREATE SCHEMA python_models"))
        await self.pool.execute(
            p.RawSQL("CREATE TYPE python_models.\"Mood\" AS ENUM ('calm', 'busy')")
        )
        await self.pool.execute(
            p.RawSQL(
                'CREATE TABLE python_models."accounts ""log" (id integer GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY, "display name" text NOT NULL DEFAULT \'default\', note text DEFAULT \'omitted\', mood python_models."Mood" NOT NULL DEFAULT \'calm\', data jsonb, tags text[])'
            )
        )
        self.model = accounts()

    async def asyncTearDown(self):
        await self.pool.execute(p.RawSQL("DROP SCHEMA python_models CASCADE"))
        await self.pool.close()

    # [spec:pgorm:req:python.models/test]
    async def test_crud_returns_declared_field_records(self):
        with patch(
            "subprocess.run",
            side_effect=AssertionError("runtime descriptor compiled a module"),
        ):
            row = (
                await self.model.insert(
                    {"display_name": "Nora", "mood": "busy", "tags": ["a", None]}
                )
                .returning()
                .one(self.pool)
            )
            self.assertIsInstance(row, p.ModelRecord)
            self.assertEqual(row["display_name"], "Nora")
            self.assertEqual(row["note"], "omitted")
            self.assertEqual(row["tags"], ["a", None])
            self.assertEqual(
                row.tagged("mood").type_name, p.TypeName("Mood", schema="python_models")
            )
            self.assertEqual(row.tagged("tags").element_type, "text")
            key = self.model.key({"id": row["id"]})
            updated = (
                await self.model.update({"note": None})
                .where_(key)
                .returning("id", "note")
                .one(self.pool)
            )
            self.assertEqual(dict(updated), {"id": row["id"], "note": None})
            selected = await self.model.find().filter(key).one(self.pool)
            self.assertEqual(selected["display_name"], "Nora")
            self.assertEqual(
                await self.model.delete().where_(key).execute(self.pool), 1
            )
            self.assertIsNone(await self.model.find().filter(key).one_opt(self.pool))
        row["tags"].append("local mutation")
        self.assertEqual(row["tags"], ["a", None])

    # [spec:pgorm:req:python.models/test]
    async def test_json_null_and_defaults_stay_distinct(self):
        omitted = await self.model.insert({}).returning().one(self.pool)
        sql_null = (
            await self.model.insert({"note": None, "data": None})
            .returning()
            .one(self.pool)
        )
        json_null = (
            await self.model.insert({"data": p.Value.json(None)})
            .returning()
            .one(self.pool)
        )
        self.assertEqual(omitted["note"], "omitted")
        self.assertIsNone(sql_null["note"])
        self.assertTrue(sql_null.tagged("data").is_null)
        self.assertFalse(json_null.tagged("data").is_null)
        self.assertIsNone(json_null["data"])
        with self.assertRaises(p.DatabaseError):
            await self.model.find().one_opt(self.pool)
        self.assertEqual(len(await self.model.find().all(self.pool)), 3)
        self.assertEqual(
            len(
                await self.model.find()
                .order_by(self.model.col("id").asc())
                .offset(1)
                .limit(1)
                .all(self.pool)
            ),
            1,
        )

    # [spec:pgorm:req:python.models/test]
    async def test_declared_types_validate_decoded_columns(self):
        await self.model.insert({}).execute(self.pool)
        for columns in (
            {"id": p.Column("i64")},
            {"note": p.Column("i32")},
            {"data": p.Column("json")},
            {"mood": p.Column(p.TypeName("Mood", schema="wrong"))},
        ):
            declaration = p.Model(self.model.table, columns)
            with self.subTest(columns=columns), self.assertRaises(p.DecodeError):
                await declaration.find().one(self.pool)
        self.assertTrue(await self.pool.ping())
        record = await self.pool.fetch_one(p.Select(p.literal(1).as_("wrong")))
        with self.assertRaises(p.DecodeError):
            p.ModelRecord(self.model, record, ("id",))

    # [spec:pgorm:req:python.models/test]
    async def test_alias_joins_and_composite_keys(self):
        for name in ("A", "B"):
            await self.model.insert({"display_name": name}).execute(self.pool)
        left, right = self.model.as_("a"), self.model.as_("b")
        rows = (
            await left.select("display_name")
            .join(right, left.col("id") == right.col("id"))
            .order_by(left.col("id").asc())
            .all(self.pool)
        )
        self.assertEqual([row["display_name"] for row in rows], ["A", "B"])
        await self.pool.execute(
            p.RawSQL(
                "CREATE TABLE python_models.pairs (a integer, b text, PRIMARY KEY (a, b))"
            )
        )
        pairs = p.Model(
            "pairs",
            {
                "a": p.Column("i32", primary_key=True),
                "b": p.Column("text", primary_key=True),
            },
            schema="python_models",
        )
        await pairs.insert({"a": 1, "b": "first"}).execute(self.pool)
        await pairs.insert({"a": 1, "b": "second"}).execute(self.pool)
        row = (
            await pairs.find().filter(pairs.key({"b": "second", "a": 1})).one(self.pool)
        )
        self.assertEqual(dict(row), {"a": 1, "b": "second"})

    # [spec:pgorm:req:python.models/test]
    async def test_declared_wire_kinds_keep_exact_tags(self):
        cases = [
            ("bool", "boolean", True),
            ("i8", 'pg_catalog."char"', 7),
            ("i16", "smallint", 8),
            ("i32", "integer", 9),
            ("i64", "bigint", 10),
            ("u32", "oid", 11),
            ("f32", "real", 1.5),
            ("f64", "double precision", 2.5),
            ("text", "text", "東京"),
            ("bytes", "bytea", b"\x00\xff"),
            ("decimal", "numeric", Decimal("123.4500")),
            ("uuid", "uuid", UUID("01234567-89ab-cdef-0123-456789abcdef")),
            ("json", "jsonb", {"ok": [True, None]}),
            ("date", "date", dt.date(2026, 9, 9)),
            ("time", "time", dt.time(12, 34, 56, 789)),
            ("datetime", "timestamp", dt.datetime(2026, 9, 9, 12, 34)),
            (
                "datetime_utc",
                "timestamptz",
                dt.datetime(2026, 9, 9, tzinfo=dt.timezone.utc),
            ),
            ("ipnetwork", "inet", "192.0.2.1/24"),
            ("mac_address", "macaddr", bytes.fromhex("001122334455")),
        ]
        ddl = ", ".join(f'"{kind}" {sql_type}' for kind, sql_type, _ in cases)
        await self.pool.execute(p.RawSQL(f"CREATE TABLE python_models.wire ({ddl})"))
        model = p.Model(
            "wire",
            {kind: p.Column(kind) for kind, _, _ in cases},
            schema="python_models",
        )
        row = (
            await model.insert({kind: value for kind, _, value in cases})
            .returning()
            .one(self.pool)
        )
        for kind, _, value in cases:
            with self.subTest(kind=kind):
                self.assertEqual(row.tagged(kind), p.Value(value, kind))
        await self.pool.execute(
            p.RawSQL('CREATE TABLE python_models.enums (moods python_models."Mood"[])')
        )
        mood = p.TypeName("Mood", schema="python_models")
        enums = p.Model(
            "enums",
            {"moods": p.Column(mood, array=True, nullable=True)},
            schema="python_models",
        )
        labels = ["calm", None, "busy"]
        row = await enums.insert({"moods": labels}).returning().one(self.pool)
        self.assertEqual(row.tagged("moods"), p.Value.array(mood, labels))
        with self.assertRaises(p.ConstructionError):
            enums.insert(
                {"moods": p.Value.array(p.TypeName("Mood", schema="wrong"), labels)}
            )
