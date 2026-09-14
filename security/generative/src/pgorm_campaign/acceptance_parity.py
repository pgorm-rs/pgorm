"""Establish Python/Rust replay parity across every grammar family.

Parity is not a pass. The question this answers is whether the public Python
module and a standalone Rust reproducer of the *same* program reach the same
observations through the same pgorm APIs; a program that both of them fail in
the same way has parity, and a program only one of them fails does not. Keeping
that separate from the oracle verdict is the point — an open finding is
reported as an open finding, and its parity is still recorded.

A family whose parity cannot be established at all is recorded as exactly that,
with the reason. Nothing here retries until it finds a family member it likes.
"""

import argparse
import asyncio
import json
from pathlib import Path
import sys
import time

from . import campaign_identity, grammar, profiles, replay
from .build import ROOT
from .comparison import InvalidOracle
from .grammar import FAMILIES

VERSION = 1

BUILD = "target/generative-build/build.json"
OUTPUT = "target/generative-parity"


def _outcome(program, python, native, comparison, reason):
    return {
        "program_sha256": program.digest,
        "python_status": python.get("status") if python else None,
        "native_status": native.get("status") if native else None,
        "parity": bool(comparison and comparison["equal"]),
        "established": comparison is not None,
        "reason": reason,
        "checks": (comparison or {}).get("checks", []),
    }


# [spec:pgorm:req:generative.acceptance]
async def attempt(family, index, *, checker, replayer, directory, root, limits, seed):
    """One family member, run through Python and then standalone Rust."""
    generated = grammar.generate(seed, index, family=family, limits=limits)
    program = generated.program
    python = await checker.run(program, timeout=30)
    where = Path(directory) / (family + "-" + str(index))
    replay.emit(program, where, root=root)
    native = None
    try:
        built = await replayer.build(where, target=Path(root) / "target", timeout=1800)
        native = await replayer.run(program, built, timeout=120)
        comparison = replay.compared(program, python, native)
        reason = "equal" if comparison["equal"] else "python and rust diverge"
    except (InvalidOracle, replay.ReplayError, RuntimeError, KeyError) as error:
        return _outcome(
            program, python, native, None, type(error).__name__ + ": " + str(error)
        )
    return _outcome(program, python, native, comparison, reason)


# [spec:pgorm:req:generative.replay-parity]
async def family(name, indices, **context):
    """Attempt a family's members in order, stopping when parity is established."""
    attempts = []
    for index in indices:
        started = time.monotonic()
        try:
            result = await attempt(name, index, **context)
        except Exception as error:
            result = {
                "program_sha256": None,
                "established": False,
                "parity": False,
                "reason": type(error).__name__ + ": " + str(error),
                "checks": [],
            }
        result.update(index=index, seconds=time.monotonic() - started)
        attempts.append(result)
        if result["established"]:
            break
    return {
        "family": name,
        "attempts": attempts,
        "established": any(item["established"] for item in attempts),
        "parity": any(item["parity"] for item in attempts),
    }


# [spec:pgorm:req:generative.replay-parity]
def assemble(results, *, identity, seconds, seed, limits, indices):
    """Fold per-family parity into a document that names what it could not do."""
    established = [item for item in results if item["established"]]
    divergent = [item for item in established if not item["parity"]]
    missing = [item["family"] for item in results if not item["established"]]
    return {
        "version": VERSION,
        "kind": "replay-parity",
        "claim": (
            "one generated program per grammar family, run through the public "
            "Python module and through a standalone Rust reproducer of the same "
            "program, compared on observations, selected pgorm API paths and "
            "final fixture state"
        ),
        "families_declared": len(FAMILIES),
        "families_attempted": len(results),
        "families_with_parity_established": len(established),
        "families_in_parity": len(established) - len(divergent),
        "families_diverging": [item["family"] for item in divergent],
        "families_without_established_parity": missing,
        "passed": bool(
            len(established) == len(results) == len(FAMILIES) and not divergent
        ),
        "identity": identity,
        "seed": seed,
        "generation_limits": limits,
        "indices_offered_per_family": list(indices),
        "seconds": seconds,
        "families": results,
        "does_not_establish": [
            "Parity is agreement between two runs of the same program. It is "
            "not evidence that either run is correct, and a program both sides "
            "get wrong identically has parity.",
            "One program per family is a family-level demonstration, not "
            "coverage of the family's operations, shapes or values.",
        ],
    }


def render(document):
    lines = [
        "replay parity v{version}".format(**document),
        "  families {families_with_parity_established}/{families_attempted} "
        "established, {families_in_parity} in parity".format(**document),
    ]
    for item in document["families"]:
        last = item["attempts"][-1] if item["attempts"] else {}
        lines.append(
            "  {}: {} ({})".format(
                item["family"],
                "parity" if item["parity"] else "NO PARITY",
                last.get("reason", "no attempt"),
            )
        )
    lines.append("  passed" if document["passed"] else "  FAILED")
    return "\n".join(lines)


# [spec:pgorm:req:generative.replay-parity]
async def execute(arguments):
    """Wire an owned fixture, one executor and one replayer, then walk families."""
    from .executor import Executor
    from .fixtures import Fixture
    from .oracles import Checker
    from .replay import Replay

    root = Path(arguments.root)
    build = json.loads(Path(arguments.build).read_text())
    profile = profiles.select(arguments.profile)
    output = Path(arguments.output)
    source = await campaign_identity.source(root)
    identity = {
        "source_sha256": source["content_sha256"],
        "revision": source["revision"],
        "dirty": source["dirty"],
        "extension_sha256": build["installed"]["native_sha256"],
    }
    indices = range(arguments.first, arguments.first + arguments.attempts)
    fixture = Fixture(output / "fixture", workers=1)
    await fixture.start()
    results = []
    started = time.monotonic()
    try:
        executor = Executor(
            fixture, worker=0, expected_native_sha256=identity["extension_sha256"]
        )
        try:
            context = {
                "checker": Checker(executor),
                "replayer": Replay(fixture, worker=0, identity=identity),
                "directory": output / "replay",
                "root": root,
                "limits": profile.grammar_limits(),
                "seed": arguments.seed,
            }
            for name in FAMILIES:
                results.append(await family(name, indices, **context))
                print(
                    "  {family}: {}".format(
                        "parity" if results[-1]["parity"] else "NO PARITY",
                        **results[-1],
                    ),
                    flush=True,
                )
        finally:
            await executor.close()
    finally:
        await fixture.close()
    return assemble(
        results,
        identity={**identity, "postgres": campaign_identity.postgres(fixture)},
        seconds=time.monotonic() - started,
        seed=arguments.seed,
        limits=profile.limits,
        indices=indices,
    )


def parse(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--build", default=str(ROOT / BUILD))
    parser.add_argument("--output", default=str(ROOT / OUTPUT))
    parser.add_argument("--profile", default="full", choices=profiles.names())
    parser.add_argument("--seed", type=int, default=20260913)
    parser.add_argument("--first", type=int, default=0)
    parser.add_argument("--attempts", type=int, default=3)
    return parser.parse_args(argv)


def main(argv=None):
    arguments = parse(argv)
    output = Path(arguments.output)
    output.mkdir(parents=True, exist_ok=True)
    document = asyncio.run(execute(arguments))
    (output / "parity-report.json").write_text(
        json.dumps(document, indent=2, sort_keys=True) + "\n"
    )
    print(render(document), flush=True)
    return 0 if document["passed"] else 1


__all__ = ["VERSION", "assemble", "attempt", "execute", "family", "main", "render"]


if __name__ == "__main__":
    sys.exit(main())
