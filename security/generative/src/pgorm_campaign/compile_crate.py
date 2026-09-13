"""Batch compatible cases into temporary crates, and remember where each landed.

A crate that links pgorm costs about ten seconds warm and over a minute cold,
so one crate per case would spend the whole budget on linking. Cases share a
crate when they are *compatible*: same verdict class, same compiler phase, same
dependency set. Verdict is the hard one — one exit status cannot mean both "this
built" and "this did not". Phase is insurance: rustc suppresses per body rather
than per crate, so typeck and borrowck rejections do coexist in practice, but a
resolution failure takes its whole module's later diagnostics with it.

Every case keeps its own line range in the rendered file. That is what turns a
pile of diagnostics back into per-case verdicts.
"""

from dataclasses import dataclass, field
from pathlib import Path

from .compile_case import CompileCase

# Shared with every emitted crate so a batch resolves against its own lock and
# against the checkout under test, never against whatever the workspace has
# moved on to.
MANIFEST = """[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
publish = false

[workspace]

[lib]
name = "{lib}"
path = "src/lib.rs"

[dependencies]
pgorm = {{ path = "{pgorm}" }}
{extra}"""

DEPENDENCIES = {"serde": 'serde = { version = "1", features = ["derive"] }'}

HEADER = (
    "//! Generated compile-suite batch. Every module below is one case.\n"
    "//!\n"
    "//! Nothing here is written by hand and nothing here is kept: the batch\n"
    "//! exists for exactly one `cargo build`, and its verdict is read from\n"
    "//! that build's diagnostics.\n"
)


class BatchError(RuntimeError):
    """A batch could not be rendered from the cases assigned to it."""


@dataclass
class Placement:
    """Where one case's source sits in the rendered crate."""

    case: CompileCase
    # 1-based, inclusive, into src/lib.rs for a source case.
    start: int = 0
    end: int = 0
    # Path prefix under src/ for a codegen case's generated tree.
    directory: str = ""

    def holds(self, file_name, line):
        if self.directory:
            return file_name.startswith("src/" + self.directory + "/")
        return self.start <= line <= self.end

    def data(self):
        value = self.case.data()
        if self.directory:
            value["directory"] = self.directory
        else:
            value["lines"] = [self.start, self.end]
        return value


@dataclass
class Batch:
    """One temporary crate and the cases compiled inside it."""

    name: str
    verdict: str
    phase: str
    needs: tuple[str, ...]
    placements: list[Placement] = field(default_factory=list)

    @property
    def cases(self):
        return [placement.case for placement in self.placements]

    def locate(self, file_name, line):
        for placement in self.placements:
            if placement.holds(file_name, line):
                return placement
        return None

    def data(self):
        return {
            "name": self.name,
            "verdict": self.verdict,
            "phase": self.phase,
            "needs": list(self.needs),
            "cases": [placement.data() for placement in self.placements],
        }


def _key(case):
    # Accepted and refused cases never share a crate: one batch is expected to
    # build and the other is expected not to, and a single exit status cannot
    # mean both.
    return (case.verdict, case.phase, case.kind, case.needs)


# [spec:pgorm:req:generative.compile-suite]
def group(cases, *, limit=24):
    """Partition cases into compatible batches, bounded in size.

    The cap keeps one syntactically catastrophic case from dragging a hundred
    others into an unreadable build, and keeps the rendered file small enough
    to read when a verdict has to be argued with.
    """
    buckets = {}
    for case in cases:
        buckets.setdefault(_key(case), []).append(case)
    batches = []
    for (verdict, phase, kind, needs), members in sorted(
        buckets.items(), key=lambda item: str(item[0])
    ):
        for index in range(0, len(members), limit):
            chunk = members[index : index + limit]
            batches.append(
                Batch(
                    name=f"{verdict}-{phase}-{kind}-{len(batches)}",
                    verdict=verdict,
                    phase=phase,
                    needs=needs,
                    placements=[Placement(case=case) for case in chunk],
                )
            )
    return batches


def module_name(case):
    return "case_" + case.id.replace("-", "_")


# [spec:pgorm:req:generative.compile-suite]
def render_source(batch):
    """Render src/lib.rs, stamping each case's line range as it is written.

    Warnings are allowed rather than denied. A compile suite judges rejections,
    and an unused import in a generated positive is not one; denying warnings
    would turn tidiness into a failing verdict.
    """
    lines = HEADER.splitlines()
    lines.append("#![allow(unused)]")
    lines.append("")
    for placement in batch.placements:
        case = placement.case
        if case.kind == "codegen":
            raise BatchError("a codegen case has no inline source: " + case.id)
        lines.append(f"// case: {case.id}")
        lines.append("pub mod " + module_name(case) + " {")
        start = len(lines) + 1
        lines.extend(case.source.splitlines())
        placement.start = start
        placement.end = len(lines)
        lines.append("}")
        lines.append("")
    return "\n".join(lines) + "\n"


def render_manifest(batch, *, crate, root):
    unknown = [name for name in batch.needs if name not in DEPENDENCIES]
    if unknown:
        raise BatchError("no manifest spelling for dependency " + unknown[0])
    extra = "\n".join(DEPENDENCIES[name] for name in batch.needs)
    return MANIFEST.format(
        name=crate,
        lib=crate.replace("-", "_"),
        # Absolute, always: a relative root is resolved against the emitted
        # crate's own directory, and a path dependency that lands on the crate
        # itself fails resolution rather than saying which path was wrong.
        pgorm=Path(root).resolve(),
        extra=extra + "\n" if extra else "",
    )


def batch_digest(batch):
    """Name a crate by what is in it, so cargo cannot reuse a stale build."""
    import hashlib

    body = "\0".join(placement.case.digest for placement in batch.placements)
    return hashlib.sha256((batch.name + "\0" + body).encode()).hexdigest()[:16]


# [spec:pgorm:req:generative.compile-suite]
def write(batch, directory, *, root, generated=None):
    """Materialise one batch as a buildable crate and report its manifest path.

    `generated` maps a codegen case id to the files its generator produced;
    each becomes a subdirectory module addressed by `#[path]`, so a tree whose
    index file is `lib.rs` is mounted as written rather than renamed.
    """
    directory = Path(directory)
    (directory / "src").mkdir(parents=True, exist_ok=True)
    crate = "pgorm-compile-" + batch_digest(batch)
    if any(placement.case.kind == "codegen" for placement in batch.placements):
        source = _render_generated(batch, directory, generated or {})
    else:
        source = render_source(batch)
    (directory / "src" / "lib.rs").write_text(source)
    (directory / "Cargo.toml").write_text(
        render_manifest(batch, crate=crate, root=root)
    )
    return {"crate": crate, "manifest": directory / "Cargo.toml"}


def _render_generated(batch, directory, generated):
    lines = HEADER.splitlines()
    lines.append("#![allow(unused)]")
    lines.append("")
    for placement in batch.placements:
        case = placement.case
        files = generated.get(case.id)
        if not files:
            raise BatchError("no generated files for codegen case " + case.id)
        folder = module_name(case)
        placement.directory = folder
        target = directory / "src" / folder
        target.mkdir(parents=True, exist_ok=True)
        names = set()
        for file in files:
            (target / file["name"]).write_text(file["content"])
            names.add(file["name"])
        index = "lib.rs" if "lib.rs" in names else "mod.rs"
        if index not in names:
            raise BatchError("generated tree has no index file: " + case.id)
        lines.append(f"// case: {case.id}")
        lines.append(f'#[path = "{folder}/{index}"]')
        lines.append(f"pub mod {folder};")
        lines.append("")
    return "\n".join(lines) + "\n"


__all__ = [
    "Batch",
    "BatchError",
    "Placement",
    "batch_digest",
    "group",
    "module_name",
    "render_manifest",
    "render_source",
    "write",
]
