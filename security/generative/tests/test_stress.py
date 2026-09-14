"""The stress harness must not be able to overstate what it demonstrated."""

import unittest

from pgorm_campaign import stress, stress_main


def _node(identity, op, **inputs):
    return {"id": identity, "op": op, "scope": "root", "inputs": inputs}


def _program(*nodes):
    return {"nodes": list(nodes)}


class Digest:
    """The one field the shard reads off a generated program."""

    def __init__(self, digest):
        self.digest = digest

    def data(self):
        return _program()


class Generated:
    def __init__(self, digest):
        self.program = Digest(digest)


def _generate(digests):
    def generate(seed, index, *, family=None, limits=None):
        value = digests[index % len(digests)]
        if value is None:
            raise ValueError("generation refused")
        return Generated(value)

    return generate


def _built(nodes=3, deferred=0, error=None):
    def build(data, p):
        return {
            "nodes_constructed": nodes,
            "nodes_deferred": deferred,
            "nodes_reachable": nodes,
            "error": error,
        }

    return build


def _arguments(**extra):
    return {
        "target": 10,
        "seed": 1,
        "workers": 2,
        "profile": "full",
        "limits": {"depth": 1},
        "native_build_identity": {"native_sha256": "a" * 64},
        **extra,
    }


def _assembled(tallies, digests, **extra):
    accumulator = stress.Digests()
    accumulator.absorb(digests)
    return stress_main.assemble(
        tallies=tallies,
        digests=accumulator,
        rounds=[],
        identities=extra.pop("identities", {"a" * 64}),
        attempts=extra.pop("attempts", []),
        seconds=2.0,
        arguments=_arguments(**extra),
    )


def _tally(**fields):
    return {**stress.blank(), **fields}


# [spec:pgorm:req:generative.acceptance/test]
class DeferralTests(unittest.TestCase):
    def test_result_instructions_are_deferred_not_failed(self):
        data = _program(
            _node("a", "expr.value"),
            _node("b", "result.value"),
            _node("c", "expr.add", left="a", right="b"),
        )
        self.assertEqual(stress.deferred(data), frozenset({"b", "c"}))

    def test_a_database_free_graph_defers_nothing(self):
        data = _program(_node("a", "expr.value"), _node("b", "expr.not", value="a"))
        self.assertEqual(stress.deferred(data), frozenset())

    def test_list_valued_inputs_propagate_deferral(self):
        data = _program(
            _node("a", "entity.result"),
            _node("b", "select.columns", columns=["a"]),
        )
        self.assertIn("b", stress.deferred(data))


# [spec:pgorm:req:generative.acceptance/test]
class DigestTests(unittest.TestCase):
    def test_repeated_programs_are_counted_once(self):
        accumulator = stress.Digests()
        self.assertEqual(accumulator.absorb(b"a" * 16 + b"b" * 16), 2)
        self.assertEqual(accumulator.absorb(b"a" * 16), 0)
        self.assertEqual(len(accumulator), 2)

    def test_truncated_digest_blocks_are_refused(self):
        with self.assertRaises(ValueError):
            stress.Digests().absorb(b"short")


# [spec:pgorm:req:generative.acceptance/test]
class ShardTests(unittest.TestCase):
    def test_generation_failures_are_counted_apart(self):
        tally, packed, attempts = stress.shard(
            1,
            range(3),
            None,
            limits=None,
            generate=_generate(["aa" * 16, None, "bb" * 16]),
            build=_built(),
        )
        self.assertEqual(tally["programs_attempted"], 3)
        self.assertEqual(tally["programs_generated"], 2)
        self.assertEqual(tally["generation_errors"], {"ValueError": 1})
        self.assertEqual(len(packed), 32)
        self.assertEqual(attempts, [])

    def test_a_construction_failure_keeps_a_sample(self):
        tally, _, _ = stress.shard(
            1,
            range(1),
            None,
            limits=None,
            generate=_generate(["cc" * 16]),
            build=_built(error={"class": "FormatError", "cause": "bad"}),
        )
        self.assertEqual(tally["programs_failing_construction"], 1)
        self.assertEqual(tally["failures"][0]["class"], "FormatError")
        self.assertEqual(tally["failures"][0]["index"], 0)

    def test_a_shard_never_reaches_a_database(self):
        tally, _, _ = stress.shard(
            1,
            range(2),
            None,
            limits=None,
            generate=_generate(["dd" * 16, "ee" * 16]),
            build=_built(deferred=1),
        )
        self.assertEqual(tally["reached_postgresql"], 0)
        self.assertEqual(tally["oracle_decided"], 0)
        self.assertEqual(tally["programs_partially_constructed"], 2)


# [spec:pgorm:req:generative.acceptance/test]
class AssemblyTests(unittest.TestCase):
    def test_a_short_run_does_not_pass(self):
        digests = b"".join(bytes([n]) * 16 for n in range(4))
        document = _assembled([_tally(programs_generated=4)], digests)
        self.assertEqual(document["distinct_generated_programs_constructed"], 4)
        self.assertFalse(document["passed"])

    def test_reaching_the_target_passes(self):
        digests = b"".join(bytes([n]) * 16 for n in range(10))
        document = _assembled([_tally(programs_generated=10)], digests)
        self.assertTrue(document["passed"])
        self.assertEqual(document["duplicate_programs_discarded"], 0)

    def test_a_second_extension_fails_the_run(self):
        digests = b"".join(bytes([n]) * 16 for n in range(10))
        document = _assembled(
            [_tally(programs_generated=10)],
            digests,
            identities={"a" * 64, "b" * 64},
        )
        self.assertFalse(document["native_build_identity_unchanged"])
        self.assertFalse(document["passed"])

    def test_a_subprocess_attempt_fails_the_run(self):
        digests = b"".join(bytes([n]) * 16 for n in range(10))
        document = _assembled(
            [_tally(programs_generated=10)], digests, attempts=["subprocess.Popen"]
        )
        self.assertFalse(document["passed"])
        self.assertEqual(document["extension_builds_during_stress"], 0)

    def test_the_document_states_no_oracle_decided(self):
        document = _assembled([_tally(programs_generated=1)], b"q" * 16)
        self.assertEqual(document["reached_postgresql"], 0)
        self.assertEqual(document["oracle_decided"], 0)
        self.assertIn("no database connection", document["database_note"])
        self.assertTrue(
            any("not live coverage" in x for x in document["does_not_establish"])
        )

    def test_no_field_totals_across_the_classes(self):
        document = _assembled([_tally(programs_generated=1)], b"r" * 16)
        for banned in ("total", "total_programs", "programs", "all"):
            self.assertNotIn(banned, document)

    def test_throughput_is_measured_not_targeted(self):
        document = _assembled([_tally(programs_generated=2)], b"s" * 16 + b"t" * 16)
        self.assertTrue(document["throughput"]["measured_not_targeted"])
        self.assertEqual(document["throughput"]["wall_clock_seconds"], 2.0)
        self.assertIn("measured, not a target", stress_main.render(document))


# [spec:pgorm:req:generative.acceptance/test]
class MergeTests(unittest.TestCase):
    def test_shard_tallies_fold_without_a_total(self):
        merged = stress.merge(
            [
                _tally(programs_attempted=2, families={"select": 2}),
                _tally(programs_attempted=3, families={"select": 1, "graph": 2}),
            ]
        )
        self.assertEqual(merged["programs_attempted"], 5)
        self.assertEqual(merged["families"], {"select": 3, "graph": 2})


if __name__ == "__main__":
    unittest.main()
