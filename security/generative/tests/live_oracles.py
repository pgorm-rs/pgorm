"""Independent-driver oracle checks in an owned disposable PostgreSQL fixture."""

import asyncio
import json
from pathlib import Path
import uuid

from pgorm_campaign.executor import Executor
from pgorm_campaign.fixtures import Fixture
from pgorm_campaign.oracles import Checker
from pgorm_campaign.program import Program

import execution_cases as cases
import execution_variants as variants
import execution_pipeline as pipelines
import oracle_cases
import oracle_pipeline_cases
import oracle_window_cases


KNOWN_DEFECTS = {
    "known-set-precedence",
    "known-nullable-count",
    "known-first-last-frame",
    "window-values-partition-False",
    "window-values-partition-True",
    "window-empty-frame-partition-False",
    "window-empty-frame-partition-True",
}


# [spec:pgorm:req:generative.oracles/test]
# [spec:pgorm:req:generative.comparison/test]
async def main():
    root = Path("target/generative-oracles")
    output = root / ("run-" + uuid.uuid4().hex[:12])
    output.mkdir(parents=True)
    summary = {"passed": False, "output": str(output), "results": []}
    (root / "summary.json").write_text(json.dumps(summary) + "\n")
    programs = [
        ("select", cases.select_case()),
        ("expressions", variants.expressions()),
        ("model", cases.model_case()),
        ("joins", variants.joins()),
        ("transaction", cases.transaction_case()),
        ("active", cases.active_case()),
        ("entity-writes", variants.entity_writes()),
        ("writes", variants.writes()),
        ("schema", cases.schema_case()),
        ("schema-changes", variants.schema_changes()),
        ("graph-filters", variants.graph_filters()),
        ("stream-all", cases.stream_case(take=8, cancel=False)),
        ("stream-prefix", cases.stream_case(take=1, cancel=False)),
        ("stream-cancel", cases.stream_case(take=0, cancel=True)),
        (
            "cursor-joined",
            cases.graph_case("campaign.OptionalNotes", ["n"], cursor=True),
        ),
    ]
    for method in (
        "contains_text",
        "starts_with",
        "ends_with",
        "like",
        "not_like",
        "ilike",
        "not_ilike",
    ):
        programs.append(("pattern-" + method, variants.patterns(method)))
    for name, program in [
        ("pipeline-stages", pipelines.stages()),
        ("pipeline-sets", pipelines.sets()),
        ("pipeline-group", pipelines.group()),
        ("pipeline-window", pipelines.window()),
    ]:
        programs.append((name, program))
    for count in range(2, 7):
        programs.append(("sources" + str(count), pipelines.sources(count)))
    for bound, sources in ((False, False), (True, False), (True, True)):
        programs.append(
            (
                f"pipeline-{bound}-{sources}",
                cases.pipeline_case(bound=bound, sources=sources),
            )
        )
    for name, aliases in [
        ("AccountOnly", []),
        ("OptionalNotes", ['n" 雪']),
        ("RequiredNotes", ["n"]),
        ("SelfJoin", ["related"]),
        *[
            ("Arity" + str(n), ["n" + str(i) for i in range(n - 1)])
            for n in range(3, 8)
        ],
    ]:
        programs.append((name, cases.graph_case("campaign." + name, aliases)))
    build = json.loads(Path("target/generative-build/build.json").read_text())
    programs.extend(oracle_cases.type_cases())
    programs.extend(oracle_cases.literal_cases())
    programs.extend(oracle_pipeline_cases.cases())
    programs.extend(oracle_window_cases.cases())
    for name in ("null-membership", "empty-membership", "empty-condition"):
        programs.append((name, oracle_cases.three_valued(name)))
    programs.append(("sqlstate", oracle_cases.rejection()))
    distinct = Program(
        Path(
            "security/generative/findings/distinct-append/minimal.program.json"
        ).read_text()
    )
    programs.append(("fixed-distinct-append-inspect", distinct))
    distinct_fetch = distinct.data()
    distinct_fetch["steps"][0].update(
        op="fetch", data={"mode": "all", "ordered": False}
    )
    programs.append(("fixed-distinct-append-fetch", Program.from_dict(distinct_fetch)))
    finding = Path(
        "security/generative/findings/set-precedence/pipeline-sets.program.json"
    )
    programs.append(("known-set-precedence", Program(finding.read_text())))
    (output / "build.json").write_text(json.dumps(build, indent=2) + "\n")
    async with Fixture(output / "fixture") as fixture:
        async with Executor(
            fixture, expected_native_sha256=build["installed"]["native_sha256"]
        ) as executor:
            checker = Checker(executor)
            for name, program in programs:
                report = await checker.run(program)
                (output / (name + ".program.json")).write_text(program.encoded + "\n")
                (output / (name + ".json")).write_text(
                    json.dumps(report, indent=2) + "\n"
                )
                result = {
                    "name": name,
                    "status": report["status"],
                    "expected": "defect"
                    if name in KNOWN_DEFECTS
                    else "expected-rejection"
                    if name == "sqlstate"
                    else "pass",
                    "error": report.get("error"),
                    "differences": [
                        item for item in report["comparisons"] if not item["equal"]
                    ],
                }
                summary["results"].append(result)
                print(json.dumps(result), flush=True)
    summary["passed"] = all(
        item["status"] == item["expected"] for item in summary["results"]
    )
    (root / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    if not summary["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    asyncio.run(main())
