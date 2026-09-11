"""Execute generated programs against independent PostgreSQL fixture copies."""

import argparse
import asyncio
import hashlib
import json
import os
from pathlib import Path
import uuid
import time

from pgorm_campaign.executor import Executor
from pgorm_campaign.fixtures import Fixture
from pgorm_campaign.grammar import FAMILIES, generate
from pgorm_campaign.grammar_state import structure
from pgorm_campaign.oracles import Checker
from pgorm_campaign.program import Program
from pgorm_campaign.sqlmap_import import load as load_corpus


def programs(count, seed, path, family=None, corpus=()):
    if path is not None:
        program = Program(path.read_bytes())
        yield program, {"family": "retained", "program_sha256": program.digest}
        return
    for index in range(count):
        selected = (
            family or (tuple(FAMILIES) + ("rejection",))[index % (len(FAMILIES) + 1)]
        )
        generated = generate(
            seed,
            index,
            family=selected,
            mode="invalid" if selected == "rejection" else "valid",
            corpus=corpus,
        )
        yield generated.program, generated.recipe()


async def main(count, seed, path=None, family=None, corpus_path=None):
    os.environ["TZ"] = "UTC"
    time.tzset()
    if type(count) is not int or not 1 <= count <= 10000:
        raise ValueError("live grammar verification requires 1..10000 programs")
    count = 1 if path is not None else count
    if path is not None and corpus_path is not None:
        raise ValueError("retained programs already contain their exact input values")
    corpus = () if corpus_path is None else load_corpus(corpus_path)
    output = Path("target/generative-grammar") / ("run-" + uuid.uuid4().hex[:12])
    output.mkdir(parents=True)
    build = json.loads(Path("target/generative-build/build.json").read_text())
    summary = {
        "passed": False,
        "seed": seed,
        "expected": count,
        "results": [],
        "native_sha256": build["installed"]["native_sha256"],
        "timezone": {"TZ": "UTC", "tzname": list(time.tzname)},
        "corpus": None
        if corpus_path is None
        else {
            "path": str(corpus_path),
            "inputs": len(corpus),
            "manifest_sha256": hashlib.sha256(
                (corpus_path / "manifest.json").read_bytes()
            ).hexdigest(),
        },
    }
    (output / "build.json").write_text(json.dumps(build, indent=2) + "\n")
    print(str(output), flush=True)
    async with Fixture(output / "fixture") as fixture:
        async with Executor(
            fixture, expected_native_sha256=summary["native_sha256"]
        ) as executor:
            checker = Checker(executor)
            for index, (program, recipe) in enumerate(
                programs(count, seed, path, family, corpus)
            ):
                (output / f"{index}.program.json").write_text(program.encoded + "\n")
                (output / f"{index}.recipe.json").write_text(
                    json.dumps(recipe, indent=2) + "\n"
                )
                report = await checker.run(program)
                (output / f"{index}.json").write_text(
                    json.dumps(report, indent=2) + "\n"
                )
                result = {
                    "index": index,
                    "production": recipe["family"],
                    "status": report["status"],
                    "expected": "expected-rejection"
                    if any(
                        item["oracle"] == "exact-error"
                        for item in program.data()["observations"]
                    )
                    else "pass",
                    "shape": structure(program),
                    "differences": [
                        item
                        for item in report.get("comparisons", [])
                        if not item["equal"]
                    ],
                    "error": report.get("error"),
                }
                summary["results"].append(result)
                if result["status"] != result["expected"]:
                    print(json.dumps(result), flush=True)
                elif (index + 1) % 100 == 0:
                    print(
                        json.dumps({"executed": index + 1, "expected": count}),
                        flush=True,
                    )
    summary["passed"] = len(summary["results"]) == count and all(
        item["status"] == item["expected"] for item in summary["results"]
    )
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(
        json.dumps(
            {
                "passed": summary["passed"],
                "executed": len(summary["results"]),
                "output": str(output),
            }
        ),
        flush=True,
    )
    if not summary["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--count", type=int, default=99)
    parser.add_argument("--seed", type=int, default=20260911)
    parser.add_argument("--program", type=Path)
    parser.add_argument("--family", choices=tuple(FAMILIES) + ("rejection",))
    parser.add_argument(
        "--corpus", type=Path, help="verified offline sqlmap import directory"
    )
    arguments = parser.parse_args()
    asyncio.run(
        main(
            arguments.count,
            arguments.seed,
            arguments.program,
            arguments.family,
            arguments.corpus,
        )
    )
