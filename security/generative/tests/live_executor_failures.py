"""Failure paths remain failed and leave the reusable runtime usable."""

import asyncio
import json
import subprocess
import sys
from unittest.mock import patch

from pgorm_campaign import resolution
from pgorm_campaign.effects import Effects
from pgorm_campaign.executor import Executor
from execution_cases import Case, select_case


def slow_case():
    c = Case()
    query = c.node(
        "raw.template",
        {"parameters": []},
        {"text": "SELECT 1::int AS value FROM pg_sleep(1)"},
    )
    c.fetch(query)
    return c.program()


def invalid_name():
    c = Case()
    name = c.node("name", {"value": c.value("text", "bad\0name")})
    table = c.node("table", {"name": name}, {"schema": "fixture"})
    column = c.node("expr.column", {"table": table}, {"name": "id"})
    query = c.node("select", {"columns": [column]})
    query = c.node("select.from", {"query": query, "table": table})
    c.fetch(query)
    return c.program()


# [spec:pgorm:req:generative.execution/test]
# [spec:pgorm:req:generative.build-amortization/test]
async def check(executor, output):
    results = {}
    invalid = await executor.run(invalid_name())
    assert invalid["status"] == "error"
    assert invalid["steps"][0]["observation"]["class"] == "ConstructionError"
    results["invalid-input"] = invalid
    timeout = await executor.run(slow_case(), timeout=0.05)
    assert (
        timeout["status"] == "incomplete"
        and timeout["error"]["class"] == "TimeoutError"
    )
    results["timeout"] = timeout
    task = asyncio.create_task(executor.run(slow_case()))
    # Wait for native acquisition rather than cancelling fixture provisioning.
    for _ in range(500):
        if executor.subject.status().available < executor.subject.status().size:
            break
        await asyncio.sleep(0.01)
    else:
        raise AssertionError("cancellation probe never acquired its native connection")
    task.cancel()
    try:
        await task
    except asyncio.CancelledError:
        pass
    else:
        raise AssertionError("cancellation did not propagate")
    with patch.object(
        resolution.dispatch_query, "dispatch", side_effect=RuntimeError("dead dispatch")
    ):
        dead = await executor.run(select_case())
        assert (
            dead["status"] == "incomplete" and "dead dispatch" in dead["error"]["cause"]
        )
        results["dead-dispatch"] = dead

    def compile_attempt(*args):
        subprocess.run([sys.executable, "-c", "pass"], check=True)

    with patch.object(
        resolution.dispatch_query, "dispatch", side_effect=compile_attempt
    ):
        attempted = await executor.run(select_case())
        assert attempted["status"] == "incomplete"
        assert attempted["subprocess_attempts"] == ["subprocess.Popen"]
        results["runtime-process"] = attempted

    async def omit_effects(self, program, graph, report):
        for node in program["nodes"]:
            graph.get(node["id"])

    with patch.object(Executor, "execute", omit_effects):
        missing = await executor.run(select_case())
        assert missing["status"] == "incomplete"
        assert "omitted" in missing["error"]["cause"]
        results["missing-effects"] = missing
    with patch.object(Effects, "perform", return_value=({"kind": "unit"}, [])):
        inactive = await executor.run(select_case())
        assert inactive["status"] == "incomplete"
        assert "no active native" in inactive["error"]["cause"]
        results["inactive-effect"] = inactive
    recovery = await executor.run(select_case())
    assert recovery["status"] == "executed" and not recovery["cleanup_errors"]
    results["recovery"] = recovery
    (output / "failure-paths.json").write_text(json.dumps(results, indent=2) + "\n")
    return sorted(results) + ["cancel-propagated"]
