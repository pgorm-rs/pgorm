"""Application statement composition against the installed native builders."""

import unittest

from pgorm import (
    Condition, Conflict, ConflictTarget, ConstructionError, Join, Nulls,
    RawSQL, Table, Value, bind, call, col, delete, insert, literal, select, update,
)


# [spec:pgorm:req:python.statements/test]
# [spec:pgorm:req:python.raw/test]
class StatementTests(unittest.TestCase):
    def test_runtime_tables_and_aliases_preserve_identity(self):
        table = Table('events"雪', schema="Tenant'A")
        alias = table.as_("e")
        self.assertEqual(alias.name, 'events"雪')
        self.assertEqual(alias.schema, "Tenant'A")
        self.assertEqual(alias.alias, "e")
        self.assertIsNone(table.alias)
        self.assertEqual(alias.col("id").inspect().sql, 'SELECT "e"."id"')
        self.assertEqual(alias.star().inspect().sql, 'SELECT "e".*')
        self.assertIn('"Tenant\'A"."events""雪" AS "e"', select(alias.star()).from_(alias).inspect().sql)
        for invalid in ["", "x" * 64, "nul\0"]:
            with self.assertRaises(ConstructionError):
                Table(invalid)

    def test_select_join_filter_group_order_and_bounds(self):
        account = Table("account", schema="application", alias="a")
        event = Table("event", schema="application", alias="e")
        count = call("count", event.col("id"))
        query = (select(account.col("name"), count.as_("events"))
                 .from_(account)
                 .join(event, account.col("id") == event.col("account_id"), kind=Join.Left)
                 .where_(Condition.all(account.col("active") == True,
                                       Condition.any(event.col("kind") == "click", event.col("id").is_null())))
                 .group_by(account.col("name"))
                 .having(count > 2)
                 .order_by(count.desc(nulls=Nulls.Last), account.col("name").asc())
                 .limit(5).offset(1))
        compiled = query.inspect()
        self.assertIn('LEFT JOIN "application"."event" AS "e" ON "a"."id" = "e"."account_id"', compiled.sql)
        self.assertIn('GROUP BY "a"."name" HAVING COUNT("e"."id") > $3', compiled.sql)
        self.assertIn('ORDER BY COUNT("e"."id") DESC NULLS LAST, "a"."name" ASC', compiled.sql)
        self.assertEqual([value.value for value in compiled.params], [True, "click", 2, 5, 1])
        self.assertEqual([value.kind for value in compiled.params[-2:]], ["u64", "u64"])
        self.assertNotIn("LIMIT", query.limit(None).offset(None).inspect().sql)
        self.assertIn("LIMIT", query.inspect().sql)

    def test_select_replacement_and_cross_join_are_explicit(self):
        left, right = Table("left"), Table("right")
        base = select().from_(left)
        selected = base.select(left.col("id")).cross_join(right).distinct()
        self.assertEqual(base.inspect().sql, 'SELECT * FROM "left"')
        self.assertEqual(selected.inspect().sql, 'SELECT DISTINCT "left"."id" FROM "left" CROSS JOIN "right"')
        with self.assertRaises(ConstructionError):
            base.select()
        with self.assertRaises(ConstructionError):
            select("untyped column")
        with self.assertRaises(TypeError):
            base.join(right)
        with self.assertRaises(TypeError):
            base.join(right, literal(True), kind="left")

    def test_limits_and_ordering_reject_ambiguous_inputs(self):
        query = select(literal(1))
        for invalid in [True, -1, 2**63, 2**64, 1.0, "10"]:
            for method in ["limit", "offset"]:
                with self.subTest(value=invalid, method=method):
                    with self.assertRaises(ConstructionError):
                        getattr(query, method)(invalid)
        self.assertEqual(query.limit(0).inspect().params, [Value(0, "u64")])
        self.assertEqual(query.limit(2**63 - 1).inspect().params, [Value(2**63 - 1, "u64")])
        with self.assertRaises(ConstructionError):
            query.order_by("name DESC")

    def test_insert_owns_rows_and_preserves_value_modes(self):
        table = Table("account", schema="application")
        base = insert(table).columns("id", "name")
        first = base.values(1, "Alice")
        second = first.values(literal(2), "O'Brien").returning(col("id"), col("name").as_("label"))
        compiled = second.inspect()
        self.assertIn('INSERT INTO "application"."account" ("id", "name") VALUES ($1, $2), (2, $3)', compiled.sql)
        self.assertIn('RETURNING "id", "name" AS "label"', compiled.sql)
        self.assertEqual(compiled.params, [Value(1), Value("Alice"), Value("O'Brien")])
        self.assertEqual(first.inspect().params, [Value(1), Value("Alice")])
        with self.assertRaises(ConstructionError):
            base.inspect()

    def test_insert_rejects_incomplete_or_inconsistent_shapes(self):
        table = Table("account")
        for build in [lambda: insert(table).inspect(),
                      lambda: insert(table).values(1),
                      lambda: insert(table).columns("id", "id"),
                      lambda: insert(table).columns(),
                      lambda: insert(table).columns("id").values(1, 2),
                      lambda: insert(table).columns("id").values(1).columns("name"),
                      lambda: insert(table).columns("id").default_values(),
                      lambda: insert(table).default_values().values()]:
            with self.assertRaises(ConstructionError):
                build()
        default = insert(table).default_values().returning().inspect()
        self.assertEqual(default.sql, 'INSERT INTO "account" VALUES (DEFAULT) RETURNING *')
        self.assertEqual(default.params, [])

    def test_conflict_targets_and_updates_follow_rust_typestate(self):
        table = Table("account", alias="a")
        target = ConflictTarget("id").where_(col("id") > 0)
        action = target.update("name").set("visits", literal(1)).where_(col("active") == True)
        query = insert(table).columns("id", "name").values(1, "Alice").on_conflict(action)
        compiled = query.inspect()
        self.assertIn('ON CONFLICT ("id") WHERE "id" > $3 DO UPDATE SET "name" = "excluded"."name", "visits" = 1', compiled.sql)
        self.assertEqual([value.value for value in compiled.params], [1, "Alice", 0, True])
        self.assertIn("DO NOTHING", query.on_conflict(target.ignore()).inspect().sql)
        self.assertIn("ON CONFLICT DO NOTHING", query.on_conflict(Conflict.ignore()).inspect().sql)
        for build in [lambda: ConflictTarget(), lambda: target.update(),
                      lambda: query.on_conflict(target)]:
            with self.assertRaises(ConstructionError):
                build()

    def test_update_and_delete_require_explicit_write_intent(self):
        table = Table("account", schema="application", alias="a")
        base = update(table).set("name", "new' name")
        with self.assertRaises(ConstructionError):
            base.inspect()
        query = base.where_(table.col("id") == 4).returning(col("id"))
        compiled = query.inspect()
        self.assertEqual(compiled.sql, 'UPDATE "application"."account" AS "a" SET "name" = $1 WHERE "a"."id" = $2 RETURNING "id"')
        self.assertEqual(compiled.params, [Value("new' name"), Value(4)])
        self.assertNotIn("WHERE", base.all_rows().inspect().sql)
        with self.assertRaises(ConstructionError):
            base.set("name", "again")
        with self.assertRaises(ConstructionError):
            update(table).all_rows().inspect()
        with self.assertRaises(ConstructionError):
            delete(table).inspect()
        self.assertEqual(delete(table).where_(table.col("id") == 4).returning().inspect().params, [Value(4)])
        self.assertNotIn("WHERE", delete(table).all_rows().inspect().sql)

    def test_raw_sql_preserves_template_and_separate_values(self):
        template = "SELECT $2::text, $1::int8, '$9', $$ $8 $$ /* $7 */"
        values = [1, "O'Brien"]
        query = RawSQL(template, values)
        values.clear()
        self.assertEqual(query.inspect().sql, template)
        self.assertEqual(query.inspect().params, [Value(1), Value("O'Brien")])
        inline = query.inline_sql()
        self.assertIn("1::int8", inline)
        self.assertIn("'$9'", inline)
        self.assertIn("$$ $8 $$", inline)
        self.assertIn("/* $7 */", inline)
        self.assertNotIn("$1::int8", inline)
        for query in [RawSQL("SELECT $2", [1]), RawSQL("SELECT 1", [1]), RawSQL("SELECT $0", [1])]:
            with self.assertRaises(ConstructionError):
                query.inline_sql()
        with self.assertRaises(ConstructionError):
            RawSQL("SELECT 1\0")
        with self.assertRaises(ConstructionError):
            RawSQL("SELECT $1", "not a parameter sequence")

    def test_statement_truth_testing_is_rejected(self):
        table = Table("account")
        for query in [select(), insert(table), update(table), delete(table), RawSQL("SELECT 1")]:
            with self.assertRaises(ConstructionError):
                bool(query)


if __name__ == "__main__":
    unittest.main()
