import copy
import unittest

from pgorm_campaign import wire
from pgorm_campaign.catalog import EFFECTS, OPERATIONS
from pgorm_campaign.grammar import FAMILIES, generate
from pgorm_campaign.grammar_pipeline import Column, Pipeline
from pgorm_campaign.grammar_state import Limits, State
from pgorm_campaign.program import Program


# [spec:pgorm:req:generative.grammar/test]
class GrammarTests(unittest.TestCase):
    def test_all_families_cover_catalog_within_budgets(self):
        operations, effects, writes, conflicts, modes = (
            set(),
            set(),
            set(),
            set(),
            set(),
        )
        for family in FAMILIES:
            for index in range(150):
                generated = generate(
                    20260911,
                    index,
                    family=family,
                    limits=Limits(depth=3, stages=5, nodes=128),
                )
                program = generated.program.data()
                self.assertLessEqual(len(program["nodes"]), 128)
                self.assertEqual(Program.from_dict(program), generated.program)
                operations.update(node["op"] for node in program["nodes"])
                effects.update(step["op"] for step in program["steps"])
                writes.update(
                    node["data"]["method"]
                    for node in program["nodes"]
                    if node["op"] == "model.write"
                )
                conflicts.update(
                    node["data"]["action"]
                    for node in program["nodes"]
                    if node["op"] == "insert.conflict"
                )
                modes.update(
                    step["data"]["mode"]
                    for step in program["steps"]
                    if step["op"] == "begin"
                )
        self.assertEqual(operations, set(OPERATIONS))
        self.assertEqual(effects, set(EFFECTS))
        self.assertEqual(writes, {"insert", "update", "delete"})
        self.assertEqual(conflicts, {"nothing", "update"})
        self.assertIn("read_only", modes)

    def test_window_functions_receive_their_public_arguments(self):
        seen = set()
        for index in range(100):
            program = generate(20260911, index, family="relational").program.data()
            if not any(n["op"] == "pipeline.window" for n in program["nodes"]):
                continue
            for node in program["nodes"]:
                if node["op"] == "pipeline.function":
                    name = node["data"]["name"]
                    seen.add(name)
                    self.assertEqual(
                        len(node["inputs"]["arguments"]),
                        0 if name == "row_number" else 1,
                    )
        self.assertTrue({"rank", "rank_dense", "row_number"} <= seen)

    def test_group_keys_leave_the_aggregate_input_scope(self):
        for index in range(150):
            program = generate(20260911, index, family="relational").program.data()
            nodes = {node["id"]: node for node in program["nodes"]}
            keys = {
                nodes[ref]["data"]["name"]
                for node in program["nodes"]
                if node["op"] == "pipeline.group"
                for ref in node["inputs"]["keys"]
            }
            if keys:
                for node in program["nodes"]:
                    if node["op"] == "pipeline.function":
                        for ref in node["inputs"]["arguments"]:
                            self.assertNotIn(nodes[ref]["data"]["name"], keys)

    def test_schema_renames_preserve_earlier_insert_columns(self):
        program = generate(20260911, 0, family="schema").program.data()
        create = next(
            node for node in program["nodes"] if node["op"] == "schema.create"
        )
        insert = next(node for node in program["nodes"] if node["op"] == "insert")
        self.assertEqual(
            insert["data"]["columns"],
            [column["name"] for column in create["data"]["columns"]],
        )

    def test_generated_programs_vary_structure_and_real_inputs(self):
        for family in ("select", "sequence", "pipeline"):
            outputs = [generate(101, index, family=family) for index in range(300)]
            self.assertEqual(len({item.program.digest for item in outputs}), 300)
            self.assertGreater(
                len({item.recipe()["structure_sha256"] for item in outputs}), 100
            )
            self.assertEqual(outputs[11], generate(101, 11, family=family))
            for output in outputs:
                self.assertEqual(
                    Program.from_dict(output.program.data()), output.program
                )
                self.assertFalse(output.recipe()["expected_rejections"])
                self.assertLessEqual(len(output.program.data()["nodes"]), 256)

    def test_budget_changes_still_produce_valid_programs(self):
        for index in range(300):
            output = generate(
                101,
                index,
                family="pipeline",
                limits=Limits(depth=3, stages=5, nodes=128),
            )
            self.assertLessEqual(len(output.program.data()["nodes"]), 128)
        for limits in ({"depth": 4}, {"stages": 0}, {"nodes": 400}, {"depth": True}):
            with self.assertRaises(ValueError):
                Limits(**limits)

    def test_rejection_profiles_require_exact_declared_errors(self):
        cases = set()
        for index in range(60):
            result = generate(17, index, mode="invalid")
            recipe = result.recipe()
            cases.add(recipe["rejection_case"])
            self.assertEqual(recipe["mode"], "invalid")
            self.assertEqual(len(recipe["expected_rejections"]), 1)
            self.assertIn(
                recipe["expected_rejections"][0]["error"]["cause"],
                {"sqlstate:22012", "sqlstate:23502", "sqlstate:23505"},
            )
        self.assertEqual(cases, {"division", "not-null", "duplicate"})
        for options in (
            {"mode": "unknown"},
            {"mode": "invalid", "family": "select"},
            {"family": "rejection"},
        ):
            with self.assertRaises(ValueError):
                generate(1, 0, **options)

    def test_sources_types_and_binder_ownership_are_checked(self):
        state = State(1, 0)
        source = state.source()
        self.assertTrue(state.column(source, "score").nullable)
        self.assertFalse(state.column(source, "id").nullable)
        with self.assertRaises(ValueError):
            state.binary(state.constant("i32", 1), state.constant("text", "1"), "eq")
        pipe = Pipeline(State(1, 0))
        with self.assertRaisesRegex(ValueError, "unavailable source"):
            pipe.expression(Column("missing", "i32"))
        result = next(
            generate(18, index, family="pipeline")
            for index in range(100)
            if generate(18, index, family="pipeline").program.data()["binders"]
        )
        data = copy.deepcopy(result.program.data())
        owner = data["binders"][0]["owner"]
        next(node for node in data["nodes"] if node["id"] == owner)["data"].pop(
            "binder"
        )
        with self.assertRaises(wire.FormatError):
            Program.from_dict(data)

    def test_reused_results_require_the_original_producer(self):
        result = generate(22, 4, family="sequence")
        data = result.program.data()
        results = [node for node in data["nodes"] if node["op"] == "result.value"]
        self.assertTrue(results)
        producer = results[0]["data"]["step"]
        consumer = next(step for step in data["steps"] if step["id"] == producer)
        consumer["op"], consumer["data"] = "execute", {}
        with self.assertRaises(wire.FormatError):
            Program.from_dict(data)
