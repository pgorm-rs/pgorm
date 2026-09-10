"""Create actual registered entity tables, indexes, enums and comments."""
import os
import unittest

import pgorm as p
from pgorm import schema as s


class RegisteredSchema(unittest.IsolatedAsyncioTestCase):
    # [spec:pgorm:req:python.schema/test]
    async def test_registered_entity_schema_executes_explicitly(self):
        account = p.entity("app.Account")
        generated = s.from_entity(account)
        self.assertEqual((len(generated.enums), len(generated.indexes), len(generated.comments)), (1, 1, 2))
        detached = generated.enums
        detached.clear()
        self.assertEqual(len(generated.enums), 1)
        async with p.Pool(os.environ["PGORM_TEST_DSN"], max_size=1) as pool:
            before = await pool.fetch_one(p.RawSQL("SELECT count(*) AS n FROM pg_namespace WHERE nspname = 'python_entities'"))
            self.assertEqual(before["n"], 0)
            await pool.execute(p.RawSQL("CREATE SCHEMA python_entities"))
            try:
                for statement in generated.enums:
                    await pool.execute(statement)
                await pool.execute(generated.table)
                for statement in generated.indexes + generated.comments:
                    await pool.execute(statement)
                async with pool.connection() as connection:
                    active = account.active().set("id", 1).set("display name", "Native").set("note", None)
                    model = await active.insert(connection)
                    self.assertEqual(model["display name"], "Native|before")
                    self.assertEqual(model["mood"], "calm")
                    self.assertEqual((await account.find().one(connection))["id"], 1)
                comments = await pool.fetch_one(p.RawSQL("SELECT obj_description('python_entities.accounts'::regclass, 'pg_class') AS table_comment, col_description('python_entities.accounts'::regclass, 2) AS column_comment"))
                self.assertEqual(dict(comments), {"table_comment": "Accounts' native schema", "column_comment": "The user's display name"})
                indexes = await pool.fetch_all(p.RawSQL("SELECT indexname FROM pg_indexes WHERE schemaname = 'python_entities' ORDER BY indexname"))
                self.assertEqual([row["indexname"] for row in indexes], ["accounts_pkey", "idx-accounts-display name"])
            finally:
                await pool.execute(p.RawSQL("DROP SCHEMA python_entities CASCADE"))


if __name__ == "__main__":
    unittest.main()
