"""Run one campaign profile against an owned PostgreSQL fixture.

    PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \\
      -m pgorm_campaign.campaign_main --profile smoke

The build is prepared separately by `pgorm_campaign.build`; this command reads
its recorded identity and refuses to run against a different extension. Every
run writes into a directory it just created, so no run can inherit an earlier
run's passing evidence.
"""

import argparse
import asyncio
import json
import os
from pathlib import Path
import sys
import time

from . import campaign_identity, campaign_plan, campaign_report, profiles
from .build import ROOT, content_identity
from .campaign_artifacts import Artifacts, fresh
from .campaign_runner import Runner
from .controls import Controls
from .executor import Executor
from .fixtures import Fixture
from .oracles import Checker

OUTPUT = "target/generative-campaign"
LEDGER = "security/generative/runs"
BUILD = "target/generative-build/build.json"


def _compile_suite(root, output):
    """The compile suite, kept behind a callable so its evidence stays separate."""
    from .compile_main import execute

    async def run():
        return await execute(
            root=root,
            target=Path("target"),
            output=output / "compile",
            limit=24,
            timeout=1800,
        )

    return run


def _cleanup(fixture, executors):
    """Cleanup is part of the run: a failure here is a fault, not an exception."""

    async def close():
        errors = []
        for executor in executors:
            try:
                await executor.close()
            except Exception as error:
                errors.append(type(error).__name__ + ": " + str(error))
        try:
            await fixture.close()
        except Exception as error:
            errors.append(type(error).__name__ + ": " + str(error))
        errors.extend(str(item) for item in fixture.report.get("cleanup_errors") or ())
        return errors

    return close


# [spec:pgorm:req:generative.verdict]
# [spec:pgorm:req:generative.artifacts]
async def execute(profile, output, build, *, root):
    """Wire the owned fixture, per-worker executors and the runner together."""
    artifacts = Artifacts(output)
    identity = await campaign_identity.collect(root, build, profile)
    artifacts.write("build.json", build)
    artifacts.write("capabilities.json", build["installed"]["capabilities"])
    plan = campaign_plan.schedule(profile)
    native = build["installed"]["native_sha256"]
    fixture = Fixture(output / "fixture", workers=profile.workers)
    executors = []
    await fixture.start()
    for worker in range(profile.workers):
        executors.append(
            Executor(fixture, worker=worker, expected_native_sha256=native)
        )
    runner = Runner(
        profile,
        plan,
        artifacts,
        checkers={index: Checker(item) for index, item in enumerate(executors)},
        controls={index: Controls(item) for index, item in enumerate(executors)},
        compile_suite=_compile_suite(root, output)
        if profile.included("compile")
        else None,
        cleanup=_cleanup(fixture, executors),
        identity=identity,
        fixture=fixture,
        root=root,
    )
    return await runner.run()


def _progress(document, output):
    print(campaign_report.render(document), flush=True)
    print(json.dumps({"passed": document["passed"], "output": str(output)}), flush=True)


class StaleBuild(Exception):
    """The installed extension was built from source other than what is here."""


# [spec:pgorm:req:generative.build-amortization]
def refuse_stale(build, root):
    """Refuse a build whose recorded source is not the tree about to be tested.

    The build records the digest it was made from, and rebuilding is already
    conditioned on that digest; nothing on the run path consulted it, so a run
    could report a fixed defect as still broken because its subject predated
    the fix. Refusing rather than rebuilding keeps a long link out of what the
    caller asked to be a read, and says what to run instead.
    """
    recorded = build.get("identity", {}).get("source_sha256")
    actual = content_identity(root)
    if recorded == actual:
        return
    raise StaleBuild(
        f"the installed extension was built from source {recorded} but this tree "
        f"is {actual}; run `python -m pgorm_campaign.build` and try again"
    )


# [spec:pgorm:req:generative.artifacts]
def record_run(summary, document, ledger):
    """Keep enough of a run outside `target` to diff the next one against it.

    A run's own artifacts live under `target`, which an ordinary `cargo clean`
    deletes — it took the 2026-09-14 full run with it, and the next run's
    ninety-two findings could not be told from the previous thirty-seven
    because the baseline no longer existed. The bulk output stays disposable,
    since the campaign is seeded, but the identity of each finding does not:
    item, verdict and program digest are what a comparison needs, and they cost
    a few kilobytes.
    """
    ledger.mkdir(parents=True, exist_ok=True)
    findings = sorted(
        (
            {
                "item": finding["item"],
                "run_class": finding["run_class"],
                "verdict": finding["verdict"],
                "program_sha256": finding["program_sha256"],
            }
            for finding in document["findings"]
        ),
        key=lambda finding: finding["item"],
    )
    entry = {
        "profile": summary["profile"],
        "passed": summary["passed"],
        "revision": document.get("source", {}).get("revision"),
        "dirty": document.get("source", {}).get("dirty"),
        "output": summary["output"],
        "counts": summary["counts"],
        "fault_kinds": summary["fault_kinds"],
        "coverage_outstanding": sorted(summary["coverage"]),
        "findings": findings,
    }
    name = Path(summary["output"]).name
    (ledger / f"{name}.json").write_text(
        json.dumps(entry, indent=2, sort_keys=True) + "\n"
    )


# [spec:pgorm:req:generative.artifacts]
async def main(arguments):
    """Create the run directory, execute the profile and report the outcome."""
    os.environ["TZ"] = "UTC"
    time.tzset()
    profile = profiles.select(arguments.profile)
    build = json.loads(Path(arguments.build).read_text())
    refuse_stale(build, Path(arguments.root))
    output = fresh(arguments.output, prefix=arguments.profile)
    print(str(output), flush=True)
    started = time.monotonic()
    try:
        document = await execute(profile, output, build, root=Path(arguments.root))
    except Exception as error:
        failure = {
            "passed": False,
            "profile": arguments.profile,
            "output": str(output),
            "seconds": time.monotonic() - started,
            "error": {"class": type(error).__name__, "cause": str(error)},
        }
        (output / "summary.json").write_text(json.dumps(failure, indent=2) + "\n")
        print(json.dumps(failure), flush=True)
        return 1
    summary = {
        "passed": document["passed"],
        "profile": arguments.profile,
        "output": str(output),
        "counts": document["counts"],
        "coverage": document["coverage"]["declared_missing"],
        "fault_kinds": document["aggregate"]["fault_kinds"],
        "seconds": document["timing"].get("total_seconds"),
    }
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    record_run(summary, document, Path(arguments.root) / arguments.ledger)
    _progress(document, output)
    return 0 if document["passed"] else 1


def parse(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", default="smoke", choices=profiles.names())
    parser.add_argument("--output", default=OUTPUT)
    parser.add_argument("--build", default=BUILD)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--ledger", default=LEDGER)
    return parser.parse_args(argv)


if __name__ == "__main__":
    sys.exit(asyncio.run(main(parse())))
