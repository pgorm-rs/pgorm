"""Run the stress demonstration and retain what it actually established.

    PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \\
      -m pgorm_campaign.stress_main --programs 1000000

Every worker loads the extension the separate build command installed, records
its content hash on entry and on exit, and refuses to start against a different
one. That is what "one unchanged native build identity" means here: not a claim
in the report but the same digest recorded by every process that did the work.

The report measures throughput and states it. It sets no target, and nothing in
this command fails because a run was slow.
"""

import argparse
from concurrent.futures import ProcessPoolExecutor
from dataclasses import asdict
import hashlib
import json
import os
from pathlib import Path
import sys
import time

from . import profiles, stress
from .build import ROOT

BUILD = "target/generative-build/build.json"
OUTPUT = "target/generative-stress"
TARGET = 1_000_000

# What a construction-only demonstration cannot be read as, stated in the
# document itself so a reader cannot arrive at the numbers without it.
LIMITS = (
    "This demonstration is construction through the native binding only. It is "
    "not live coverage: no program here reached PostgreSQL and none was decided "
    "by an oracle, so it neither replaces the full runtime campaign nor proves "
    "any ORM program safe.",
    "Distinctness is program-digest distinctness at the recorded generation "
    "limits. It is not a claim that a million distinct behaviours were covered.",
    "Throughput is a measurement of this machine on this run. No speed target "
    "is imposed and no run fails for being slow.",
)


def _native_sha256():
    """The content hash of the extension the worker actually imported."""
    import pgorm._native as native

    return hashlib.sha256(Path(native.__file__).read_bytes()).hexdigest()


def _work(job):
    """One shard, run in its own process against its own loaded extension."""
    seed, start, count, limits, expected = job
    import pgorm as p

    entry = _native_sha256()
    if expected is not None and entry != expected:
        raise RuntimeError("stress worker loaded a different extension")
    started = time.monotonic()
    tally, digests, attempts = stress.shard(
        seed, range(start, start + count), p, limits=limits
    )
    tally["seconds"] = time.monotonic() - started
    return {
        "tally": tally,
        "digests": digests,
        "subprocess_attempts": attempts,
        "native_sha256_entry": entry,
        "native_sha256_exit": _native_sha256(),
        "pid": os.getpid(),
        "start": start,
        "count": count,
    }


def _jobs(seed, start, count, limits, expected, chunk):
    """Split a contiguous index range into shards of at most `chunk` programs."""
    return [
        (seed, at, min(chunk, start + count - at), limits, expected)
        for at in range(start, start + count, chunk)
    ]


def _round(pool, jobs, distinct, digests):
    """Run one batch of shards, folding tallies and distinct digests as they land."""
    tallies, identities, attempts = [], set(), []
    added = 0
    for result in pool.map(_work, jobs):
        tallies.append(result["tally"])
        identities.update((result["native_sha256_entry"], result["native_sha256_exit"]))
        attempts.extend(result["subprocess_attempts"])
        added += digests.absorb(result["digests"])
    distinct.append(
        {
            "shards": len(jobs),
            "programs_attempted": sum(t["programs_attempted"] for t in tallies),
            "distinct_added": added,
            "distinct_after": len(digests),
        }
    )
    return tallies, identities, attempts


def _identity(build, revision):
    """The single build identity every shard must have been running against."""
    identity = build.get("identity", {})
    installed = build.get("installed", {})
    return {
        "native_sha256": installed.get("native_sha256"),
        "source_sha256": identity.get("source_sha256"),
        "build_revision": identity.get("revision"),
        "checkout_revision": revision,
        "rustc": identity.get("rustc"),
        "interpreter": identity.get("interpreter"),
        "wheel_sha256": build.get("wheel_sha256"),
        "builds_total_before_stress": build.get("builds_total"),
    }


# [spec:pgorm:req:generative.acceptance]
# [spec:pgorm:req:generative.build-amortization]
def assemble(*, tallies, digests, rounds, identities, attempts, seconds, arguments):
    """Build the retained stress document out of what the shards reported."""
    totals = stress.merge(tallies)
    observed = sorted(identities)
    expected = arguments["native_build_identity"]["native_sha256"]
    unchanged = observed == [expected]
    distinct = len(digests)
    document = {
        "version": stress.VERSION,
        "kind": "stress-demonstration",
        "claim": stress.CLAIM,
        "target_distinct_programs": arguments["target"],
        "distinct_generated_programs_constructed": distinct,
        "programs_attempted": totals["programs_attempted"],
        "programs_generated": totals["programs_generated"],
        "duplicate_programs_discarded": totals["programs_generated"] - distinct,
        "reached_postgresql": totals["reached_postgresql"],
        "oracle_decided": totals["oracle_decided"],
        "database_note": (
            "zero programs reached PostgreSQL and zero were decided by an "
            "oracle; this run opened no database connection at all"
        ),
        "native_build_identity": arguments["native_build_identity"],
        "native_sha256_observed": observed,
        "native_build_identity_unchanged": unchanged,
        "extension_builds_during_stress": 0,
        "per_program_builds": 0,
        "subprocess_attempts": attempts,
        "build_guard": (
            "every shard constructed inside the audit hook that refuses "
            "subprocesses, so no build tool could run per program"
        ),
        "throughput": {
            "wall_clock_seconds": seconds,
            "distinct_programs_per_second": distinct / seconds if seconds else None,
            "attempted_programs_per_second": totals["programs_attempted"] / seconds
            if seconds
            else None,
            "worker_processes": arguments["workers"],
            "measured_not_targeted": True,
        },
        "construction": {
            key: totals[key]
            for key in (
                "programs_fully_constructed",
                "programs_partially_constructed",
                "programs_failing_construction",
                "nodes_constructed",
                "nodes_deferred",
            )
        },
        "deferred_note": (
            "instructions reading an executed effect's result cannot be "
            "constructed without a database and are counted as deferred, "
            "never as failures"
        ),
        "generation_errors": totals["generation_errors"],
        "construction_errors": totals["construction_errors"],
        "construction_failure_samples": totals["failures"],
        "families": totals["families"],
        "seed": arguments["seed"],
        "generation_limits": arguments["limits"],
        "profile_limits_from": arguments["profile"],
        "rounds": rounds,
        "does_not_establish": list(LIMITS),
    }
    document["passed"] = bool(
        distinct >= arguments["target"] and unchanged and not attempts
    )
    return document


def render(document):
    """A short human summary; the JSON document remains the evidence."""
    throughput = document["throughput"]
    return "\n".join(
        [
            "stress demonstration v{version}".format(**document),
            "  distinct generated programs constructed: "
            "{distinct_generated_programs_constructed} "
            "(target {target_distinct_programs})".format(**document),
            "  attempted {programs_attempted}, duplicates discarded "
            "{duplicate_programs_discarded}".format(**document),
            "  reached PostgreSQL {reached_postgresql}, oracle decided "
            "{oracle_decided}".format(**document),
            "  native identity {native_sha256_observed}, unchanged "
            "{native_build_identity_unchanged}".format(**document),
            "  builds during stress {extension_builds_during_stress}, "
            "subprocess attempts {n}".format(
                **document, n=len(document["subprocess_attempts"])
            ),
            "  wall clock {:.1f}s, {:.1f} distinct programs/s over {} workers "
            "(measured, not a target)".format(
                throughput["wall_clock_seconds"],
                throughput["distinct_programs_per_second"],
                throughput["worker_processes"],
            ),
            "  construction errors: "
            + (json.dumps(document["construction_errors"]) or "{}"),
            "  passed" if document["passed"] else "  FAILED",
        ]
    )


# [spec:pgorm:req:generative.acceptance]
def execute(arguments):
    """Schedule shards in rounds until the distinct target is actually reached."""
    build = json.loads(Path(arguments.build).read_text())
    profile = profiles.select(arguments.profile)
    limits = profile.grammar_limits()
    expected = build["installed"]["native_sha256"]
    if _native_sha256() != expected:
        raise RuntimeError("the loaded extension is not the recorded campaign build")
    digests, rounds, tallies = stress.Digests(), [], []
    identities, attempts = set(), []
    cursor, started = 0, time.monotonic()
    with ProcessPoolExecutor(max_workers=arguments.workers) as pool:
        while len(digests) < arguments.programs:
            outstanding = arguments.programs - len(digests)
            # Collisions run near one program in eight hundred, so a follow-up
            # round asks for a margin rather than trickling one index at a time.
            count = outstanding if not rounds else outstanding + 64 + outstanding // 8
            jobs = _jobs(
                arguments.seed, cursor, count, limits, expected, arguments.chunk
            )
            cursor += count
            batch, seen, tried = _round(pool, jobs, rounds, digests)
            tallies.extend(batch)
            identities.update(seen)
            attempts.extend(tried)
            print(render_round(rounds[-1]), flush=True)
    return assemble(
        tallies=tallies,
        digests=digests,
        rounds=rounds,
        identities=identities,
        attempts=attempts,
        seconds=time.monotonic() - started,
        arguments={
            "target": arguments.programs,
            "seed": arguments.seed,
            "workers": arguments.workers,
            "profile": arguments.profile,
            "limits": asdict(limits),
            "native_build_identity": _identity(build, arguments.revision),
        },
    )


def render_round(entry):
    return (
        "  round: {shards} shards, {programs_attempted} attempted, "
        "{distinct_added} new, {distinct_after} distinct".format(**entry)
    )


def revision():
    """The checkout the stress ran from, recorded beside the build's own."""
    import subprocess

    found = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        check=False,
    )
    return found.stdout.strip() or None


def parse(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--programs", type=int, default=TARGET)
    parser.add_argument("--seed", type=int, default=20260913)
    parser.add_argument("--profile", default="full", choices=profiles.names())
    parser.add_argument("--workers", type=int, default=os.cpu_count() or 1)
    parser.add_argument("--chunk", type=int, default=20000)
    parser.add_argument("--build", default=str(ROOT / BUILD))
    parser.add_argument("--output", default=str(ROOT / OUTPUT))
    parser.add_argument("--revision", default=None)
    arguments = parser.parse_args(argv)
    if arguments.revision is None:
        arguments.revision = revision()
    return arguments


def main(argv=None):
    arguments = parse(argv)
    output = Path(arguments.output)
    output.mkdir(parents=True, exist_ok=True)
    document = execute(arguments)
    (output / "stress-report.json").write_text(
        json.dumps(document, indent=2, sort_keys=True) + "\n"
    )
    print(render(document), flush=True)
    return 0 if document["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
