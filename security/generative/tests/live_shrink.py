"""Reduce a retained failing program against live independent PostgreSQL checks."""

import argparse
import asyncio
import json
import os
from pathlib import Path
import time
import uuid

from pgorm_campaign import shrink
from pgorm_campaign.executor import Executor
from pgorm_campaign.fixtures import Fixture
from pgorm_campaign.grammar import FAMILIES, generate
from pgorm_campaign.oracles import Checker
from pgorm_campaign.program import Program


def source(path, seed, index, family):
    """Take the exact retained program, or regenerate one by seed and index."""
    if path is not None:
        program = Program(path.read_bytes())
        return program, {"origin": "retained", "path": str(path)}
    generated = generate(seed, index, family=family)
    return generated.program, {
        "origin": "generated",
        "seed": seed,
        "index": index,
        "family": family,
    }


# [spec:pgorm:req:generative.shrink/test]
async def main(path, seed, index, family, candidates, seconds, timeout):
    os.environ["TZ"] = "UTC"
    time.tzset()
    program, origin = source(path, seed, index, family)
    budget = shrink.Budget(candidates=candidates, seconds=seconds, timeout=timeout)
    output = Path("target/generative-shrink") / ("run-" + uuid.uuid4().hex[:12])
    output.mkdir(parents=True)
    build = json.loads(Path("target/generative-build/build.json").read_text())
    (output / "build.json").write_text(json.dumps(build, indent=2) + "\n")
    (output / "original.program.json").write_text(program.encoded + "\n")
    print(str(output), flush=True)
    summary = {"passed": False, "origin": origin}
    async with Fixture(output / "fixture") as fixture:
        async with Executor(
            fixture, expected_native_sha256=build["installed"]["native_sha256"]
        ) as executor:
            checker = Checker(executor)
            baseline = await checker.run(program, timeout=timeout)
            (output / "original.json").write_text(json.dumps(baseline, indent=2) + "\n")
            if baseline["status"] not in shrink.FAILING:
                summary["reason"] = (
                    "the program did not reproduce a failure to shrink: "
                    + baseline["status"]
                )
                (output / "summary.json").write_text(
                    json.dumps(summary, indent=2) + "\n"
                )
                print(json.dumps(summary), flush=True)
                raise SystemExit(1)
            result = await shrink.reduce(
                checker,
                program,
                budget=budget,
                report=baseline,
                observer=lambda item: print(json.dumps(item), flush=True),
            )
            report = result.report()
            confirmation = await checker.run(result.best, timeout=timeout)
    (output / "best.program.json").write_text(result.best.encoded + "\n")
    (output / "best.json").write_text(json.dumps(confirmation, indent=2) + "\n")
    (output / "shrink.json").write_text(json.dumps(report, indent=2) + "\n")
    summary.update(
        {
            "native_sha256": build["installed"]["native_sha256"],
            "predicate": report["predicate"],
            "budget": report["budget"],
            "original": report["original"],
            "best": report["best"],
            "reduced": report["reduced"],
            "offered": report["offered"],
            "executed": report["executed"],
            "accepted": report["accepted"],
            "interrupted": report["interrupted"],
            "exhausted": report["exhausted"],
            "seconds": report["seconds"],
            "globally_minimal": False,
            "confirmed": result.predicate.holds(confirmation),
            "output": str(output),
        }
    )
    # A reduced reproducer is only worth keeping if a final independent run from
    # its own baseline still shows the frozen failure.
    summary["passed"] = summary["confirmed"]
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary), flush=True)
    if not summary["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--program", type=Path)
    parser.add_argument("--seed", type=int, default=20260911)
    parser.add_argument("--index", type=int, default=0)
    parser.add_argument("--family", choices=tuple(FAMILIES))
    parser.add_argument("--candidates", type=int, default=120)
    parser.add_argument("--seconds", type=float, default=1800.0)
    parser.add_argument("--timeout", type=float, default=10.0)
    arguments = parser.parse_args()
    asyncio.run(
        main(
            arguments.program,
            arguments.seed,
            arguments.index,
            arguments.family,
            arguments.candidates,
            arguments.seconds,
            arguments.timeout,
        )
    )
