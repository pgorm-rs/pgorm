"""Emit standalone reproducers and check them against the independent oracle."""

import hashlib
import json
import time
from pathlib import Path

from . import baseline, emit_python, emit_rust, process
from .attribution import parity
from .comparison import InvalidOracle, encoded
from .oracles import compare
from .program import Program
from .reference import Driver, Reference
from .reference_inspection import inspect_reads
from .reference_state import snapshot

VERSION = 1

# The emitted crate resolves against its own lock rather than the workspace's,
# so a reproducer keeps building after the workspace moves on.
#
# Its name carries the program identity. Reproducers share one target
# directory, and cargo keys its freshness on the package name: a fixed name
# would let a second reproducer inherit the first one's binary and report a
# stale success, which is the one failure a compile check must never have.
CRATE = """[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
publish = false

[workspace]

[[bin]]
name = "{name}"
path = "src/main.rs"

[dependencies]
pgorm-generative-replay = {{ path = "{replay}" }}
pgorm = {{ path = "{pgorm}" }}
serde_json = "1"
tokio = {{ version = "1", features = ["rt-multi-thread", "macros"] }}
"""


class ReplayError(RuntimeError):
    """A reproducer could not be emitted, built or run as retained."""


def _relative(target, start):
    return Path(target).resolve().relative_to(Path(start).resolve(), walk_up=True)


# [spec:pgorm:req:generative.replay]
def emit(program, directory, *, root=Path(".")):
    """Write the portable program plus Python and Rust reproducers for it.

    The program itself is retained verbatim alongside the sources: a seed is
    not evidence, and neither is a reproducer nobody can tie back to the
    recorded program and its concrete fixture.
    """
    if not isinstance(program, Program):
        raise ReplayError("emitting a reproducer requires a validated Program")
    directory = Path(directory)
    (directory / "rust" / "src").mkdir(parents=True, exist_ok=True)
    root = Path(root).resolve()
    crate = "pgorm-reproducer-" + program.digest[:16]
    documents = {
        "program.json": program.encoded + "\n",
        "fixture.json": encoded(program.data()["fixture"]) + "\n",
        "python.py": emit_python.render(program),
        "rust/src/main.rs": emit_rust.render(program),
        "rust/Cargo.toml": CRATE.format(
            name=crate,
            replay=_relative(root / "security/generative/replay", directory / "rust"),
            pgorm=_relative(root, directory / "rust"),
        ),
    }
    hashes = {}
    for name, text in documents.items():
        data = text.encode()
        (directory / name).write_bytes(data)
        hashes[name] = hashlib.sha256(data).hexdigest()
    manifest = {
        "version": VERSION,
        "program_sha256": program.digest,
        "crate": crate,
        "files": dict(sorted(hashes.items())),
    }
    (directory / "manifest.json").write_text(encoded(manifest) + "\n")
    return manifest


def _digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class _Owner:
    """What the inspection probe needs from an executor, and nothing more."""

    def __init__(self, fixture, worker):
        self.fixture = fixture
        self.worker = worker


# [spec:pgorm:req:generative.replay]
class Replay:
    """Run an emitted Rust reproducer as the subject of the ordinary oracle.

    The reproducer is the subject and only the subject. The independent
    semantics stay in Python, so the Rust side is measured against the same
    reference the binding is, rather than against a second implementation that
    could agree with it and both be wrong.
    """

    def __init__(self, fixture, *, worker=0, identity):
        self.fixture = fixture
        self.worker = worker
        self.identity = identity

    async def build(self, directory, *, target=Path("target"), timeout=900):
        """Compile the emitted crate once and record what was actually built.

        The build shares the repository target directory on purpose. A private
        one makes every reproducer a cold build of pgorm and its 293 resolved
        packages; the measured difference in this tree is 77 seconds against
        10.4, and nothing about a reproducer needs its own copy of the rlibs.

        Resolution is pinned and offline: a reproducer that quietly picked up a
        newer dependency would no longer be the thing that was recorded.
        """
        manifest = Path(directory) / "rust" / "Cargo.toml"
        lock = Path(directory) / "rust" / "Cargo.lock"
        target = Path(target)
        crate = json.loads((Path(directory) / "manifest.json").read_text())["crate"]
        if not lock.exists():
            await process.run(
                "cargo",
                "generate-lockfile",
                "--offline",
                "--manifest-path",
                manifest,
                timeout=timeout,
            )
        await process.run(
            "cargo",
            "build",
            "--quiet",
            "--locked",
            "--offline",
            "--manifest-path",
            manifest,
            "--target-dir",
            target,
            timeout=timeout,
        )
        executable = target / "debug" / crate
        if not executable.exists():
            raise ReplayError("the emitted reproducer did not compile")
        return {
            "executable": executable,
            "executable_sha256": _digest(executable),
            "lock_sha256": _digest(lock),
        }

    async def run(self, program, built, *, timeout=120):
        """Reset the paired fixtures, run the reproducer, and check it."""
        started = time.monotonic()
        data = program.data()
        report = {
            "program_sha256": program.digest,
            "status": "incomplete",
            "comparisons": [],
            "cleanup_errors": [],
        }
        try:
            # A reproducer has to stand on its own, so the schema is rebuilt
            # rather than merely restored: nothing carries over from whatever
            # ran before it.
            await self.fixture.reset(self.worker, data["fixture"], rebuild=True)
            pair = self.fixture.pair(self.worker)
            result = await process.run(
                built["executable"],
                timeout=timeout,
                check=False,
                # The DSN carries a generated password; it travels in the
                # environment and is never written to any retained document.
                environment={
                    "PGORM_REPLAY_URL": pair.subject,
                    "PATH": "/usr/bin:/bin",
                    "TZ": "UTC",
                },
            )
            report["provenance"] = {
                "backend": "standalone-rust",
                "exit_code": result.returncode,
                "timed_out": False,
                "source_sha256": self.identity["source_sha256"],
                "executable_sha256": built["executable_sha256"],
                "lock_sha256": built["lock_sha256"],
            }
            if result.returncode != 0:
                raise ReplayError("the reproducer exited " + str(result.returncode))
            report["subject"] = _subject(result.stdout, program)
            await inspect_reads(
                _Owner(self.fixture, self.worker), program, report["subject"]
            )
            async with await Driver.connect(
                self.fixture, worker=self.worker
            ) as reference:
                report["reference"] = await Reference(reference).run(
                    program, timeout=timeout
                )
                report["reference_state"] = await snapshot(reference)
            async with await Driver.connect(
                self.fixture, worker=self.worker, side="subject"
            ) as subject:
                report["subject_state"] = await snapshot(subject)
            report["comparisons"] = compare(
                data,
                report["subject"],
                report["reference"],
                report["subject_state"],
                report["reference_state"],
            )
            if not all(item["equal"] for item in report["comparisons"]):
                report["status"] = "defect"
            elif any(item["oracle"] == "exact-error" for item in report["comparisons"]):
                report["status"] = "expected-rejection"
            else:
                report["status"] = "pass"
        except Exception as error:
            report["error"] = {"class": type(error).__name__, "cause": str(error)}
        finally:
            report["seconds"] = time.monotonic() - started
        return report


def _subject(stdout, program):
    """Read the reproducer's own report, refusing anything it did not say."""
    try:
        subject = json.loads(stdout)
    except ValueError as error:
        raise ReplayError("the reproducer did not print a report: " + str(error))
    if not isinstance(subject, dict):
        raise ReplayError("the reproducer printed something other than a report")
    if subject.get("program_sha256") != program.digest:
        raise ReplayError("the reproducer reported another program")
    expected = [step["id"] for step in program.data()["steps"]]
    if [step.get("id") for step in subject.get("steps", ())] != expected:
        raise ReplayError("the reproducer omitted or reordered scheduled effects")
    return subject


# [spec:pgorm:req:generative.replay-parity]
def compared(program, python, native):
    """Compare a Python run and an emitted Rust run of the same program."""
    checks = parity(program, python, native)
    return {
        "version": VERSION,
        "program_sha256": program.digest,
        "fixture_sha256": baseline.digest(program.data()["fixture"]),
        "checks": checks,
        "equal": all(check["equal"] for check in checks),
        "python_status": python.get("status"),
        "native_status": native.get("status"),
    }


def families(program):
    """Which grammar families a program's instructions actually exercise."""
    from .catalog import EFFECTS, OPERATIONS

    seen = {OPERATIONS[node["op"]].family for node in program.data()["nodes"]}
    return sorted(
        seen | {EFFECTS[step["op"]].family for step in program.data()["steps"]}
    )


__all__ = [
    "InvalidOracle",
    "Replay",
    "ReplayError",
    "VERSION",
    "compared",
    "emit",
    "families",
]
