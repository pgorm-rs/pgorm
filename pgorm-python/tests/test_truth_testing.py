"""Truth testing across every class the installed package defines."""

import importlib
import pkgutil
import unittest

import pgorm as p
from pgorm import _native, pipeline as pl, schema as s
from pgorm._registered import entities as views, graphs as graph_views, versions as version_views
from pgorm.codegen.types import FieldType
from pgorm.models import Column, Model, ModelRecord
from pgorm.runtime import PoolStatus

# Classes whose instances keep Python's own truth, grouped by why that is safe.
EXEMPT = {
    # Values: data compared with ==, which answers a Python bool, so `and`,
    # `or` and `if` over them stay ordinary Python. A multirange is false when
    # it holds no range, as any container is.
    p.Value, p.Range, p.Multirange, p.CreatedRange, p.CreatedMultirange, p.Interval,
    p.TypeName, p.Identifier,
    # Rows, models and their write state, and what a write answers: data read
    # after SQL ran. A record or a model is a mapping, false with no fields.
    p.Record, p.Field, p.EntityModel, p.ActiveModel, p.ActiveValue, p.ActiveState,
    p.Change, p.Upserted, ModelRecord, PoolStatus, views.ModelView, views.ActiveView,
    version_views.Change, version_views.Inserted, version_views.Updated,
    # Pools, connections, transactions and the private native handles under
    # them and under the streams: live resources, not SQL, and true as any
    # open resource is. The streams themselves refuse, since a stream's truth
    # would say nothing of whether rows remain.
    p.Pool, p.Connection, p.Transaction,
    _native._Pool, _native._Connection, _native._Transaction, _native._Stream,
    _native._CopyIn, _native._CopyOut, _native._Listener,
    # Registrations and declarations: they name what a query reads and are
    # true as any declared object is; the queries they start refuse.
    p.Entity, p.EntityGraph, Model, Column, pl.SourceSelection, pl.Binder,
    views.EntityView, graph_views.GraphView, FieldType,
}

# Refusing classes only a registered application or a live server can build:
# registered_entities.py, registered_graphs.py, registered_sources.py,
# registered_schema.py, test_listen.py, test_streams.py and test_copy.py
# truth-test their instances.
BUILT_ELSEWHERE = {
    p.EntityColumn, p.EntityQuery, p.EntityUpdate, p.EntityUpdateMany, p.EntityInsert,
    p.EntityInsertMany, p.EntityGraphQuery, p.EntityGraphCursor, pl.SelectedSources, s.EntitySchema,
    views.QueryView, graph_views.GraphQueryView, graph_views.GraphCursorView,
    version_views.UpdateView, version_views.UpdateManyView, version_views.InsertView,
    version_views.InsertManyView, p.Notification, p.Listener, p.RowStream,
    p.CopyOutStream, p.CopyRowStream,
}


def defined_classes():
    """Every class a module of the package defines or exports."""
    found = set()
    for module in pkgutil.walk_packages(p.__path__, "pgorm."):
        if module.name.endswith("__main__"):
            continue
        for value in vars(importlib.import_module(module.name)).values():
            if isinstance(value, type) and value.__module__.split(".")[0] == "pgorm":
                found.add(value)
    # An exception is true as every Python exception is; it carries no SQL.
    return {kind for kind in found if not issubclass(kind, BaseException)}


def statement_samples():
    table, source = p.Table("account"), p.Table("staged")
    query = p.select(p.col("id")).from_(table)
    pending = p.merge(table, source, table.col("id") == source.col("id"))
    rows = p.FrameType.Rows
    ranked = p.window_function("rank")
    return [
        table, query, p.insert(table), p.update(table), p.delete(table), p.RawSQL("SELECT 1"),
        query.inspect(), pending, pending.when_matched(p.MatchedAction.Delete),
        p.TableFunction(p.call("generate_series", 1, 3)),
        p.TableFunction(p.call("generate_series", 1, 3)).alias("series"),
        p.GroupingElement.empty(), p.Join.Inner, p.SampleMethod.Bernoulli,
        p.With("recent", query), p.ReturningRow.Old,
        p.Conflict.do_nothing(), p.ConflictTarget("id"), p.ConflictTarget("id").update("name"),
        p.MatchedAction.Delete, p.NotMatchedAction.DoNothing, p.Overriding.SystemValue,
        p.MergeUpdate("name", "x"), p.MergeInsert("id", 1),
        rows, p.FrameExclusion.Ties, rows.preceding(1), rows.current_row(), rows.following(1),
        rows.preceding(1).and_current_row(), p.Window(), p.ExtendedWindow("w"),
        ranked, ranked.over(p.Window()),
        p.CopyFrom(table, "text"), p.CopyTo(table, "csv"),
    ]


def expression_samples():
    expression = p.col("id") > 1
    return [
        expression, p.Condition.all(expression), expression.as_("big"), expression.asc(),
        p.LikePattern("a%"), p.Direction.Asc, p.Nulls.First, p.format_json(p.col("doc")),
        p.JsonKind.Object, p.JsonExistsBehavior.Error, p.JsonValueBehavior.Null,
        p.JsonQueryBehavior.Null, p.JsonDefault("x"), p.JsonTableBehavior.Empty,
        p.JsonTableColumn.ordinality("n"), p.any_(p.col("ids")),
        _native._DeclaredColumn(p.col("id")),
    ]


def other_samples():
    table = p.Table("items")
    model = Model("items", {"id": Column("i32", primary_key=True)})
    return [
        s.DataType("integer"), s.ColumnDef("id", "integer"), s.create_table(table),
        s.create_index(table, "id"), s.drop_table(table), s.create_domain("d", "integer"),
        pl.from_(table), pl.col("items", "id"), pl.from_(table).group(pl.col("items", "id")),
        pl.Over(), pl.source(table),
        model.col("id"), model.find(), model.delete(), model.insert({"id": 1}).returning("id"),
    ]


class TruthTesting(unittest.TestCase):
    # [spec:pgorm:req:python.truth-testing+3/test]
    def test_every_query_class_refuses_truth_testing(self):
        samples = statement_samples() + expression_samples() + other_samples()
        for sample in samples:
            with self.subTest(sample=sample), self.assertRaises(p.ConstructionError):
                bool(sample)
        # A class the package adds must refuse and be sampled above, or be
        # listed as exempt with its reason, so none evaluates as true unnoticed.
        defined = defined_classes()
        sampled = {type(sample) for sample in samples}
        self.assertEqual(defined - sampled - EXEMPT - BUILT_ELSEWHERE, set())
        self.assertEqual((EXEMPT | BUILT_ELSEWHERE) - defined, set())
        self.assertEqual(sampled & EXEMPT, set())
        for kind in BUILT_ELSEWHERE:
            with self.subTest(kind=kind):
                self.assertIn("__bool__", vars(kind))

    # [spec:pgorm:req:python.truth-testing+3/test]
    def test_exempt_values_keep_python_truth(self):
        self.assertTrue(p.Value(0))
        self.assertTrue(p.Identifier("id"))
        self.assertFalse(p.Multirange([]))
        self.assertTrue(p.Multirange([p.Range(1, 2)]))
        self.assertTrue(p.ConstructionError("refused"))


if __name__ == "__main__":
    unittest.main()
