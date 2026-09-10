"""Explicit DDL from the installed native wheel, with no implicit database work."""
import importlib
import os
import unittest
from uuid import uuid4

import pgorm as p
from pgorm import schema as s


class SchemaConstruction(unittest.TestCase):
    # [spec:pgorm:req:python.schema/test]
    def test_capabilities_and_detached_builders(self):
        manifest = p.capabilities()
        self.assertFalse(manifest["schema_policy"]["automatic_ddl"])
        self.assertFalse(manifest["operations"]["schema.create_table"]["registration_required"])
        self.assertTrue(manifest["operations"]["schema.from_entity"]["registration_required"])
        for operation in manifest["operations"]:
            if operation.startswith("schema."):
                p.require_capability(operation)
        table = s.create_table(p.Table("test"))
        column = s.ColumnDef("id", "integer")
        built = table.column(column.not_null()).primary_key("id")
        self.assertNotIn('"id"', table.inspect().sql)
        self.assertIn('"id" integer NOT NULL', built.inspect().sql)
        self.assertNotIn("NOT NULL", table.column(column).inspect().sql)
        self.assertEqual(built.inspect().params, [])
        with self.assertRaises(p.UnsupportedCapabilityError):
            s.from_entity(p.Model("test", {"id": p.Column("i32")}))

    # [spec:pgorm:req:python.schema/test]
    def test_all_declared_types_retain_native_identity(self):
        for kind in p.capabilities()["schema_policy"]["column_types"]:
            with self.subTest(kind=kind):
                data_type = s.DataType(kind, length=16) if kind == "varbit" else s.DataType(kind)
                ddl = s.create_table(p.Table("test")).column(s.ColumnDef("v", data_type))
                self.assertTrue(ddl.inspect().sql.startswith('CREATE TABLE "test"'))
                self.assertEqual(ddl.inspect().params, [])
        kind = p.TypeName('Mood "雪"', schema='schema "雪"')
        sql = s.create_table(p.Table("test")).column(s.ColumnDef("v", s.DataType(kind).array())).inspect().sql
        self.assertIn('"schema ""雪"""."Mood ""雪"""[]', sql)

    # [spec:pgorm:req:python.schema/test]
    def test_invalid_options_and_unquoted_fragments_fail(self):
        for action in (
            lambda: s.DataType("text; DROP TABLE test"),
            lambda: s.DataType("text", length=4),
            lambda: s.DataType("numeric", scale=2),
            lambda: s.DataType("numeric", precision=0),
            lambda: s.DataType("numeric", precision=10, scale=1001),
            lambda: s.DataType("varchar", length=True),
            lambda: s.DataType("varchar", length=0),
            lambda: s.DataType(p.TypeName("custom"), length=3),
            lambda: s.create_table(p.Table("test", alias="t")),
            lambda: s.create_index(p.Table("test", alias="t"), "v"),
            lambda: s.drop_table(p.Table("test", alias="t")),
            lambda: s.create_index(p.Table("test"), "v").method("gin); SELECT 1 --"),
            lambda: s.ColumnDef("", "text"),
            lambda: s.ColumnDef("v", "text").generated("raw SQL"),
            lambda: s.create_enum("mood", ["a\0b"]),
            lambda: s.create_enum("mood", ["雪" * 22]),
            lambda: s.add_enum_value("mood", "v", before="a", after="b"),
        ):
            with self.subTest(action=action), self.assertRaises(p.PgOrmError):
                action()


class SchemaDatabase(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
        self.namespace = 'ddl "雪 ' + uuid4().hex[:8]
        self.quoted = '"' + self.namespace.replace('"', '""') + '"'
        await self.pool.execute(p.RawSQL(f"CREATE SCHEMA {self.quoted}"))

    async def asyncTearDown(self):
        await self.pool.execute(p.RawSQL(f"DROP SCHEMA {self.quoted} CASCADE"))
        await self.pool.close()

    # [spec:pgorm:req:python.schema/test]
    async def test_explicit_table_index_and_alter_execution(self):
        model = p.Model('items "x"', {"id": p.Column("i32", primary_key=True), "name": p.Column("text")}, schema=self.namespace)
        importlib.reload(s)
        table = model.table
        ddl = s.create_table(table).column(s.ColumnDef("id", "integer").primary_key().auto_increment())
        ddl = ddl.column(s.ColumnDef("name", "text").not_null().default("O'Brien \\ 雪"))
        ddl = ddl.column(s.ColumnDef("twice", "integer").generated(p.col("id") * 2)).check(p.col("id") > 0)
        ddl.inspect()
        count = await self.pool.fetch_one(p.RawSQL("SELECT count(*) AS n FROM information_schema.tables WHERE table_schema = $1", [self.namespace]))
        self.assertEqual(count["n"], 0)
        await self.pool.execute(ddl)
        await self.pool.execute(ddl.if_not_exists())
        inserted = await self.pool.fetch_one(p.insert(table).default_values().returning(p.col("id"), p.col("name"), p.col("twice")))
        self.assertEqual(dict(inserted), {"id": 1, "name": "O'Brien \\ 雪", "twice": 2})
        async with self.pool.connection() as connection:
            index = s.create_index(table, "name", name='idx "雪"').column("id", descending=True).nulls_not_distinct()
            await connection.execute(index)
            await connection.execute(index.if_not_exists())
            await connection.execute(s.add_column(table, s.ColumnDef("extra", "text").default("hello")))
            await connection.execute(s.add_column(table, s.ColumnDef("extra", "text"), if_not_exists=True))
            await connection.execute(s.modify_column(table, s.ColumnDef("extra", "varchar").not_null().default("changed")))
            await connection.execute(s.rename_column(table, "extra", 'renamed "x"'))
            await connection.execute(s.drop_column(table, 'renamed "x"'))
            await connection.execute(s.drop_index(table, 'idx "雪"'))
            await connection.execute(s.drop_index(table, 'idx "雪"', if_exists=True))
            await connection.execute(s.truncate(table))
            self.assertEqual(await connection.fetch_all(p.select(p.col("id")).from_(table)), [])
            await connection.execute(s.rename_table(table, 'renamed "table"'))
            renamed = p.Table('renamed "table"', schema=self.namespace)
            await connection.execute(s.drop_table(renamed))
            await connection.execute(s.drop_table(renamed, if_exists=True))

    # [spec:pgorm:req:python.schema/test]
    async def test_qualified_enum_arrays_and_label_escaping(self):
        # A same-named public type must not capture a qualified column reference.
        shadow_name = 'Mood shadow ' + uuid4().hex[:8]
        shadow = p.TypeName(shadow_name, schema="public")
        kind = p.TypeName(shadow_name, schema=self.namespace)
        await self.pool.execute(s.create_enum(shadow, ["wrong"]))
        try:
            values = ["", "O'Brien \\ 雪", "busy"]
            await self.pool.execute(s.create_enum(kind, values))
            table = p.Table('enum "items"', schema=self.namespace)
            ddl = s.create_table(table).column(s.ColumnDef("mood", kind).default(p.Value(values[1], kind)))
            ddl = ddl.column(s.ColumnDef("moods", s.DataType(kind).array()))
            await self.pool.execute(ddl)
            query = p.insert(table).columns("moods").values(p.Value.array(kind, [values[0], None, values[2]])).returning(p.col("mood"), p.col("moods"))
            row = await self.pool.fetch_one(query)
            self.assertEqual(row["mood"], values[1])
            self.assertEqual(row["moods"], ["", None, "busy"])
            self.assertEqual(row.tagged("mood").type_name, kind)
            self.assertEqual(row.tagged("moods").element_type, kind)
            await self.pool.execute(s.add_enum_value(kind, "first", before=""))
            await self.pool.execute(s.add_enum_value(kind, "last", after="busy"))
            await self.pool.execute(s.rename_enum_value(kind, "busy", "calm"))
            # Rust's driver caches type metadata on physical connections.
            # A schema migration finishes before application pools restart.
            await self.pool.close()
            self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
            changed = await self.pool.fetch_one(p.select(p.col("moods")).from_(table))
            self.assertEqual(changed["moods"], ["", None, "calm"])
            await self.pool.execute(s.rename_enum(kind, 'Renamed "雪"'))
            await self.pool.close()
            self.pool = p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1)
            renamed = p.TypeName('Renamed "雪"', schema=self.namespace)
            row = await self.pool.fetch_one(p.select(p.col("mood")).from_(table))
            self.assertEqual(row.tagged("mood").type_name, renamed)
            await self.pool.execute(s.drop_table(table))
            await self.pool.execute(s.drop_enum(renamed))
            await self.pool.execute(s.drop_enum(renamed, if_exists=True))
        finally:
            await self.pool.execute(s.drop_enum(shadow, if_exists=True, cascade=True))


if __name__ == "__main__":
    unittest.main()
