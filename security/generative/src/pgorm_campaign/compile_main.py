"""Run the compile suite end to end and retain its report.

    PYTHONPATH=security/generative/src python3 -m pgorm_campaign.compile_main

Generation happens first and separately: the codegen cases have to visit
`pgorm-codegen` before any of their source exists, and a case the generator
refused has no crate to build. Only what the generator produced reaches rustc.
"""

import argparse
import asyncio
import json
from pathlib import Path
import sys
import time

from . import compile_driver, compile_report, compile_suite
from .build import ROOT, content_identity
from .compile_runner import run


# [spec:pgorm:req:generative.compile-suite]
async def execute(*, root, target, output, limit, timeout):
    started = time.monotonic()
    suite = compile_suite.cases()
    source, codegen = compile_suite.split(suite)
    executable = await compile_driver.build(root=root, target=target, timeout=timeout)
    outcomes = await compile_driver.generate(executable, codegen, timeout=timeout)

    generated = {}
    results = []
    buildable = []
    for case in codegen:
        verdict = compile_driver.classify(case, outcomes[case.id])
        if verdict["status"] == "generated":
            generated[case.id] = outcomes[case.id]["files"]
            buildable.append(case)
        else:
            # A refusal — expected or not — is terminal for the case: there is
            # no source, so there is nothing for a compiler to say about it.
            results.append({**case.data(), **verdict, "codes": []})

    batches = await run(
        list(source) + buildable,
        output / "crates",
        root=root,
        target=target,
        generated=generated,
        limit=limit,
        timeout=timeout,
    )
    for batch in batches:
        results.extend(batch["results"])
    document = compile_report.assemble(
        results,
        compile_suite.coverage(results),
        batches,
        seconds=time.monotonic() - started,
        identity={"source_sha256": content_identity(Path(root))},
    )
    return document


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--target", default="target")
    parser.add_argument("--output", default="target/generative-compile")
    parser.add_argument("--limit", type=int, default=24)
    parser.add_argument("--timeout", type=int, default=1800)
    args = parser.parse_args(argv)

    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    document = asyncio.run(
        execute(
            root=Path(args.root),
            target=Path(args.target),
            output=output,
            limit=args.limit,
            timeout=args.timeout,
        )
    )
    (output / "compile-report.json").write_text(
        json.dumps(document, indent=2, sort_keys=True) + "\n"
    )
    print(compile_report.render(document))
    return 0 if document["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
