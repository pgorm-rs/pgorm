import copy
import json
import unittest

from pgorm_campaign import matrix, program, wire

from pgorm_campaign.baseline import default


def node(identity, operation, inputs=None, data=None, scope="root"):
    return {
        "id": identity,
        "op": operation,
        "inputs": inputs or {},
        "data": data or {},
        "scope": scope,
    }


def observe(value):
    value["observations"] = [
        {"step": step["id"], "oracle": "reference"} for step in value["steps"]
    ]
    value["observations"].append({"step": "final", "oracle": "fixture-state"})
    return value


def sample():
    return observe(
        {
            "version": 1,
            "capability_version": 1,
            "seed": 123,
            "fixture": default(),
            "binders": [],
            "nodes": [
                node("v", "value", data={"value": wire.scalar("i32", "7")}),
                node("e", "expr.value", {"value": "v"}, {"mode": "bound"}),
                node("a", "expr.alias", {"value": "e"}, {"name": 'value" 雪'}),
                node("q", "select", {"columns": ["a"]}),
            ],
            "steps": [
                node("read", "fetch", {"query": "q"}, {"mode": "all", "ordered": True})
            ],
        }
    )


def bound_sample():
    value = sample()
    value["binders"] = [{"id": "b", "owner": "q"}]
    value["nodes"] = [
        node("t", "table", data={"schema": "fixture", "name": "accounts"}),
        node("p", "pipeline.from", {"source": "t"}),
        node("c", "pipeline.column", data={"source": "accounts", "column": "id"}),
        node("v", "value", data={"value": wire.scalar("i32", "7")}),
        node("bval", "pipeline.bind", {"value": "v"}, scope="b"),
        node(
            "pred",
            "pipeline.binary",
            {"left": "c", "right": "bval"},
            {"operator": "gt"},
            scope="b",
        ),
        node(
            "q", "pipeline.filter", {"query": "p", "predicate": "pred"}, {"binder": "b"}
        ),
    ]
    return value


# [spec:pgorm:def:generative.program/test]
# [spec:pgorm:req:generative.format/test]
class ProgramTests(unittest.TestCase):
    def test_roundtrip_preserves_graph_and_owns_data(self):
        value = sample()
        artifact = program.Program.from_dict(value)
        self.assertEqual(artifact, program.Program(artifact.encoded.encode()))
        self.assertEqual(value, artifact.data())
        original = artifact.digest
        value["nodes"][0]["data"]["value"]["data"] = "9"
        artifact.data()["nodes"].clear()
        self.assertEqual(artifact.digest, original)
        self.assertEqual(artifact.data()["nodes"][0]["data"]["value"]["data"], "7")

    def test_malformed_and_unbounded_inputs_fail(self):
        for data in (
            b'{"version":1,"version":1}',
            b'{"x":NaN}',
            b"[]",
            b'"' + b"x" * program.MAX_BYTES + b'"',
            b"[" * 1000,
        ):
            with self.subTest(data=data[:60]), self.assertRaises(wire.FormatError):
                program.Program(data)
        for change in (
            lambda v: v.update(version=2),
            lambda v: v.update(capability_version=True),
            lambda v: v.update(seed=-1),
            lambda v: v.update(steps=[]),
            lambda v: v["nodes"][0].update(op="execute_arbitrary_code"),
            lambda v: v["nodes"][1]["inputs"].update(value="missing"),
            lambda v: v["nodes"][1]["inputs"].update(value="q"),
            lambda v: v["steps"][0]["inputs"].update(query="v"),
            lambda v: v.update(observations=[]),
            lambda v: v["nodes"].append(
                node("dead", "value", data={"value": wire.scalar("bool", True)})
            ),
        ):
            value = sample()
            change(value)
            with self.subTest(change=change), self.assertRaises(wire.FormatError):
                program.Program.from_dict(value)

    def test_constructor_cannot_skip_validation(self):
        with self.assertRaises(wire.FormatError):
            program.Program('{"unvalidated":true}')

    def test_write_type_is_independent_of_key_order(self):
        value = sample()
        value["nodes"] = [
            node("t", "table", data={"schema": "fixture", "name": "accounts"}),
            node("c", "expr.column", {"table": "t"}, {"name": "id"}),
            node("v", "value", data={"value": wire.scalar("i32", "1")}),
            node(
                "pred", "expr.binary", {"left": "c", "right": "v"}, {"operator": "eq"}
            ),
            node("d", "delete", {"table": "t"}),
            node("q", "write.filter", {"query": "d", "predicate": "pred"}),
        ]
        value["steps"] = [node("write", "execute", {"query": "q"})]
        artifact = program.Program.from_dict(observe(value))
        self.assertEqual(artifact, program.Program(artifact.encoded))

    def test_binder_ownership_survives_roundtrip(self):
        value = bound_sample()
        artifact = program.Program.from_dict(value)
        self.assertEqual(artifact, program.Program(artifact.encoded))
        for change in (
            lambda v: v["nodes"][4].update(scope="root"),
            lambda v: v["nodes"][5].update(scope="root"),
            lambda v: v["nodes"][6]["data"].clear(),
            lambda v: v["binders"][0].update(owner="p"),
        ):
            changed = copy.deepcopy(value)
            change(changed)
            with self.assertRaises(wire.FormatError):
                program.Program.from_dict(changed)

    def test_transaction_stack_reserves_parent(self):
        value = sample()
        begin = {"child": "tx", "mode": "default", "isolation": "default"}
        value["steps"] = [
            node("start", "begin", data=begin),
            value["steps"][0],
            node("end", "rollback", scope="tx"),
        ]
        value["steps"][1]["scope"] = "tx"
        observe(value)
        program.Program.from_dict(value)
        value["steps"][1]["scope"] = "root"
        with self.assertRaises(wire.FormatError):
            program.Program.from_dict(value)
        value["steps"][1]["scope"] = "tx"
        value["steps"].pop()
        observe(value)
        with self.assertRaises(wire.FormatError):
            program.Program.from_dict(value)

    def test_result_references_require_prior_row_effects(self):
        value = sample()
        value["nodes"].extend(
            [
                node(
                    "r",
                    "result.value",
                    data={
                        "step": "read",
                        "row": 0,
                        "column": 'value" 雪',
                        "type": {"kind": "i32"},
                    },
                ),
                node("re", "expr.value", {"value": "r"}, {"mode": "bound"}),
                node("rq", "select", {"columns": ["re"]}),
            ]
        )
        value["steps"].append(
            node("reread", "fetch", {"query": "rq"}, {"mode": "all", "ordered": True})
        )
        program.Program.from_dict(observe(value))
        value["steps"].reverse()
        with self.assertRaises(wire.FormatError):
            program.Program.from_dict(value)

    def test_stored_identifier_references_remain_data(self):
        value = sample()
        value["nodes"][0]["data"]["value"] = wire.scalar("text", 'odd" 雪')
        value["nodes"].extend(
            [
                node(
                    "r",
                    "result.value",
                    data={
                        "step": "read",
                        "row": 0,
                        "column": 'value" 雪',
                        "type": {"kind": "text"},
                    },
                ),
                node("name", "name", {"value": "r"}),
                node("table", "table", {"name": "name"}, {"schema": "fixture"}),
                node("column", "expr.column", {"table": "table"}, {"name": "id"}),
                node("select", "select", {"columns": ["column"]}),
                node("from", "select.from", {"query": "select", "table": "table"}),
            ]
        )
        value["steps"].append(
            node("reuse", "fetch", {"query": "from"}, {"mode": "all", "ordered": True})
        )
        artifact = program.Program.from_dict(observe(value))
        self.assertEqual(artifact, program.Program(artifact.encoded))


# [spec:pgorm:req:generative.matrix/test]
class MatrixTests(unittest.TestCase):
    def test_catalog_operations_have_named_paths_and_obligations(self):
        value = matrix.load()
        self.assertEqual(len(value["registered_sources"]), 6)
        required = matrix.obligations()
        self.assertIn("operation.entity.predicate", required)
        self.assertIn("operation.graph.cursor", required)
        self.assertIn("pipeline.nested-source", required)
        self.assertIn("sequences.store-read-identifier", required)
        self.assertIn("graph.arity-7", required)
        self.assertIn("crud.empty-batch", required)
        self.assertEqual(json.loads(json.dumps(value)), value)


if __name__ == "__main__":
    unittest.main()
