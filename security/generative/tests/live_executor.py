"""Owned PostgreSQL integration for the reusable native instruction executor."""

import asyncio
import json
from pathlib import Path
import uuid

import pgorm as p

from pgorm_campaign.executor import Executor
from pgorm_campaign.fixtures import Fixture
import execution_cases as cases
import execution_variants as variants
import execution_pipeline as pipelines
import live_executor_failures


def field(record, name):
    if record["kind"] == "tuple":
        record = record["items"][0]
    return next(
        item["value"]["data"] for item in record["fields"] if item["name"] == name
    )


def rows(report, index=-1):
    return report["steps"][index]["observation"]["rows"]


async def checked(executor, program, output, name):
    report = await executor.run(program)
    (output / (name + ".program.json")).write_text(program.encoded + "\n")
    (output / (name + ".json")).write_text(json.dumps(report, indent=2) + "\n")
    assert report["status"] == "executed", (name, report)
    assert report["builds"] == 0 and report["subprocess_attempts"] == []
    return report


# [spec:pgorm:req:generative.execution/test]
# [spec:pgorm:req:generative.build-amortization/test]
async def main():
    root = Path("target/generative-executor")
    output = root / ("run-" + uuid.uuid4().hex[:12])
    output.mkdir(parents=True, exist_ok=True)
    (root / "summary.json").write_text(
        json.dumps({"passed": False, "state": "running", "output": str(output)}) + "\n"
    )
    build = json.loads(Path("target/generative-build/build.json").read_text())
    reports = []
    async with Fixture(output / "fixture") as fixture:
        async with Executor(
            fixture, expected_native_sha256=build["installed"]["native_sha256"]
        ) as executor:
            for index in range(2):
                result = await checked(
                    executor, cases.select_case(), output, "select" + str(index)
                )
                assert {field(row, "id") for row in rows(result)} == {"1", "2", "4"}
                reports.append(result)
                if index == 0:
                    subject, reference = executor.subject, executor.reference
                else:
                    assert (
                        subject is executor.subject and reference is executor.reference
                    )
            for name, aliases, count in [
                ("AccountOnly", [], 4),
                ("OptionalNotes", ['n" 雪'], 5),
                ("RequiredNotes", ["n"], 3),
                ("SelfJoin", ["related"], 4),
                *[
                    (
                        "Arity" + str(arity),
                        ["n" + str(i) for i in range(arity - 1)],
                        2 ** (arity - 1) + 3,
                    )
                    for arity in range(3, 8)
                ],
            ]:
                result = await checked(
                    executor,
                    cases.graph_case("campaign." + name, aliases),
                    output,
                    name,
                )
                assert len(rows(result)) == count, (name, rows(result))
                reports.append(result)
            result = await checked(
                executor,
                cases.graph_case("campaign.OptionalNotes", ["n"], cursor=True),
                output,
                "cursor",
            )
            assert len(rows(result)) == 2
            reports.append(result)
            result = await checked(executor, variants.joins(), output, "joins")
            assert {
                (field(row, "account"), field(row, "note")) for row in rows(result)
            } == {("1", "11"), ("1", "12"), ("3", "13")}
            reports.append(result)
            result = await checked(
                executor, variants.graph_filters(), output, "graph-filters"
            )
            assert [field(row, "id") for row in rows(result)] == ["2", "3"]
            reports.append(result)
            result = await checked(
                executor, pipelines.stages(), output, "pipeline-stages"
            )
            assert {field(row, "result") for row in rows(result, 0)} == {"-2", "-3"}
            assert len(rows(result)) == 2
            reports.append(result)
            result = await checked(executor, pipelines.sets(), output, "pipeline-sets")
            assert [len(rows(result, i)) for i in range(3)] == [8, 4, 0]
            reports.append(result)
            result = await checked(
                executor, pipelines.group(), output, "pipeline-group"
            )
            assert [field(row, "total") for row in rows(result)] == ["7"]
            reports.append(result)
            result = await checked(
                executor, pipelines.window(), output, "pipeline-window"
            )
            assert {
                (field(row, "id"), field(row, "number")) for row in rows(result)
            } == {("1", "1"), ("2", "2"), ("3", "1"), ("4", "3")}
            reports.append(result)
            for arity in range(2, 7):
                result = await checked(
                    executor, pipelines.sources(arity), output, "sources" + str(arity)
                )
                assert len(rows(result)) == 2 ** (arity - 1) + 3
                assert all(len(row["items"]) == arity for row in rows(result))
                reports.append(result)
            for bound, sources in ((False, False), (True, False), (True, True)):
                result = await checked(
                    executor,
                    cases.pipeline_case(bound=bound, sources=sources),
                    output,
                    f"pipeline-{bound}-{sources}",
                )
                expected = {"1", "2", "4"} if sources else {"11", "12", "14"}
                assert {field(row, "id") for row in rows(result)} == expected
                if bound:
                    assert any(
                        "pgorm::pipeline::Binder::bind" in event.get("native_paths", [])
                        for event in result["trace"]
                    )
                reports.append(result)
            result = await checked(executor, cases.model_case(), output, "model")
            assert {field(row, "identity") for row in rows(result)} == {
                "1",
                "2",
                "3",
                "4",
            }
            assert field(rows(result)[0], "labels")[1]["sql_null"]
            reports.append(result)
            result = await checked(executor, cases.active_case(), output, "active")
            assert field(rows(result)[0], "name") == "O'Brien|updated"
            assert field(rows(result)[0], "rank") == "2"
            untouched = await executor.reference.fetch_one(
                p.RawSQL("SELECT name, rank FROM fixture.accounts WHERE id=1")
            )
            assert untouched["name"] == "Alice" and untouched["rank"] == 1
            reports.append(result)
            result = await checked(
                executor, cases.transaction_case(), output, "transaction"
            )
            assert {field(row, "body") for row in rows(result)} == {
                "first",
                "second",
                "protected",
                "orphan",
            }
            reports.append(result)
            result = await checked(executor, cases.schema_case(), output, "schema")
            assert field(rows(result, 1)[0], "id") == "8"
            reports.append(result)
            for take, cancel in ((8, False), (1, False), (0, True)):
                result = await checked(
                    executor,
                    cases.stream_case(take=take, cancel=cancel),
                    output,
                    f"stream-{take}-{cancel}",
                )
                assert len(rows(result)) == 4
                state = result["steps"][0]["observation"]["stream"]
                assert state["closed"] and state["complete"] == (take == 8)
                if cancel:
                    assert state["cancelled"]
                reports.append(result)
            assert executor.programs == len(reports)
            result = await checked(
                executor, variants.expressions(), output, "expressions"
            )
            assert field(rows(result)[0], 'peak" 雪') == "2"
            assert result["steps"][0]["observation"]["kind"] == "compiled"
            reports.append(result)
            for method in ("contains_text", "starts_with", "like", "ilike"):
                result = await checked(
                    executor, variants.patterns(method), output, "pattern-" + method
                )
                assert [field(row, "id") for row in rows(result)] == ["2"]
                reports.append(result)
            result = await checked(executor, variants.writes(), output, "writes")
            assert result["steps"][2]["observation"]["value"] == 1
            assert result["steps"][4]["observation"]["value"] == 1
            assert field(rows(result, 3)[0], "body") == "O'Brien -- 雪"
            assert (
                field(rows(result)[0], "value")
                == field(rows(result)[0], "repeated")
                == "O'Brien"
            )
            assert field(rows(result)[0], "quoted") == "$9"
            reports.append(result)
            result = await checked(
                executor, variants.entity_writes(), output, "entity-writes"
            )
            assert (
                rows(result) == [] and result["steps"][2]["observation"]["value"] == 1
            )
            reports.append(result)
            result = await checked(
                executor, variants.schema_changes(), output, "schema-changes"
            )
            assert len(rows(result)) == 1 and field(rows(result)[0], "value") is None
            reports.append(result)
            failure_paths = await live_executor_failures.check(executor, output)
    assert fixture.report["passed"] and fixture.report["state"] == "closed"
    summary = {
        "passed": True,
        "executor_probes": len(reports),
        "runtime_builds": 0,
        "failure_paths": failure_paths,
        "native_sha256": reports[0]["native_sha256"],
        "operations_observed": sorted(
            {event["operation"] for report in reports for event in report["trace"]}
        ),
        "effects_observed": sorted(
            {event["operation"] for report in reports for event in report["steps"]}
        ),
        "fixture_cleanup": "passed",
    }
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    (root / "summary.json").write_text(
        json.dumps({**summary, "output": str(output)}, indent=2) + "\n"
    )
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    asyncio.run(main())
