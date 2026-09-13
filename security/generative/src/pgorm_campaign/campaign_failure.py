"""Retention for a failing item: the original, the reduction and how to re-run it.

A failure is only useful if someone else can reach it. What is kept is the
portable program as executed, the independent observations that condemned it,
the fixture definition it declared, a reduced reproducer when the budget
allowed one, emitted Python and Rust sources, and concrete argument lists that
re-run each of those. None of it contains a connection string: the fixture
definition is synthetic data and the generated password never leaves the
process that minted it.
"""

from pathlib import Path

from . import attribution, campaign_artifacts, replay, shrink
from .comparison import InvalidOracle

PYTHON = "target/generative-build/venv/bin/python"
DRIVER = "security/generative/tests/live_campaign.py"


# [spec:pgorm:req:generative.artifacts]
def commands(directory, *, python=PYTHON):
    """Concrete argument lists, not prose instructions, for reproducing this."""
    directory = Path(directory)
    return {
        "python_replay": [python, str(directory / "replay/python.py")],
        "rust_replay": [
            "cargo",
            "run",
            "--offline",
            "--locked",
            "--manifest-path",
            str(directory / "replay/rust/Cargo.toml"),
        ],
        "recheck_program": [
            python,
            "security/generative/tests/live_grammar.py",
            "--program",
            str(directory / "replay/program.json"),
        ],
        "shrink_program": [
            python,
            "security/generative/tests/live_shrink.py",
            "--program",
            str(directory / "replay/program.json"),
        ],
        "rerun_profile": [python, DRIVER, "--profile", "smoke"],
    }


# [spec:pgorm:req:generative.artifacts]
async def reduce_program(checker, program, report, budget):
    """Reduce a failing program, recording why when it cannot be reduced."""
    if checker is None or not budget.get("enabled"):
        return None, {"attempted": False, "reason": "shrinking disabled by the profile"}
    limits = shrink.Budget(
        candidates=budget["candidates"],
        seconds=budget["seconds"],
        passes=budget["passes"],
        timeout=budget["timeout"],
        offered=budget["offered"],
    )
    try:
        result = await shrink.reduce(checker, program, budget=limits, report=report)
    except shrink.ShrinkError as error:
        return None, {"attempted": True, "reason": str(error)}
    return result.best, result.report()


# [spec:pgorm:req:generative.artifacts]
def emit_reproducers(directory, program, reduced, *, root):
    """Emit portable program plus Python and Rust sources for both versions."""
    manifests = {
        "original": replay.emit(program, Path(directory) / "replay", root=root)
    }
    if reduced is not None and reduced.digest != program.digest:
        manifests["reduced"] = replay.emit(
            reduced, Path(directory) / "reduced", root=root
        )
    return manifests


# [spec:pgorm:req:generative.artifacts]
# [spec:pgorm:req:generative.verdict]
async def retain(
    directory, record, program, report, identity, *, checker, budget, root
):
    """Write the whole retained failure and return what a reader can find."""
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=False)
    artifacts = campaign_artifacts.Artifacts(directory)
    reduced, reduction = await reduce_program(checker, program, report, budget)
    manifests = emit_reproducers(directory, program, reduced, root=root)
    argv = commands(directory)
    artifacts.write("record.json", record)
    artifacts.write("oracle.json", report)
    artifacts.write("reproducers.json", manifests)
    if reduction is not None:
        artifacts.write("shrink.json", reduction)
    result = {
        "item": record.get("id"),
        "verdict": record.get("verdict"),
        "program_sha256": program.digest,
        "reduced_program_sha256": None if reduced is None else reduced.digest,
        "reduced": reduced is not None and reduced.digest != program.digest,
        "commands": argv,
        "files": artifacts.manifest(),
    }
    try:
        result["attribution"] = attribution.retain(
            directory / "attribution", program, report, identity, commands=argv
        )
    except (InvalidOracle, OSError) as error:
        result["attribution"] = {"classification": "unattributed", "reason": str(error)}
    artifacts.write("finding.json", result)
    result["artifact_faults"] = artifacts.recheck()
    return result


__all__ = ["commands", "emit_reproducers", "reduce_program", "retain"]
