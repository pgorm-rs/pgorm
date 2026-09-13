"""Everything a report needs to say which code, pins and database it ran on.

A revision alone is not an identity for a working tree that has uncommitted
edits, so the content hash the native build already computes travels with it,
along with an explicit dirty flag and the paths responsible. Nothing here
reads a connection string: the fixture's own report carries settings and
image identity and deliberately omits credentials.
"""

from . import (
    campaign_artifacts,
    catalog,
    compile_report,
    control_catalog,
    corpus,
    grammar_state,
    matrix,
    process,
    profiles,
    program,
    replay,
    shrink,
)

VERSION = 1


# [spec:pgorm:req:generative.artifacts]
async def source(root):
    """Revision plus the dirty-content identity that makes it meaningful."""
    from .build import content_identity

    revision = (
        await process.run("git", "rev-parse", "HEAD", cwd=str(root))
    ).stdout.strip()
    status = await process.run(
        "git", "status", "--porcelain", "--untracked-files=no", cwd=str(root)
    )
    changed = [line[3:] for line in status.stdout.splitlines() if line.strip()]
    return {
        "revision": revision,
        "dirty": bool(changed),
        "dirty_paths": sorted(changed)[:64],
        "dirty_path_count": len(changed),
        "content_sha256": content_identity(root),
    }


# [spec:pgorm:req:generative.artifacts]
def versions(profile):
    """Every versioned artifact a verdict depends on, named separately."""
    document, digest = profiles.load()
    return {
        "campaign_identity": VERSION,
        "instruction_catalog": catalog.VERSION,
        "program_format": program.VERSION,
        "grammar": grammar_state.VERSION,
        "corpus": corpus.VERSION,
        "coverage_matrix": matrix.load()["version"],
        "control_catalog": control_catalog.VERSION,
        "compile_report": compile_report.VERSION,
        "shrink": shrink.VERSION,
        "replay": replay.VERSION,
        "profile_document": document["version"],
        "profile_document_sha256": digest,
        "profile": profile.name,
        "profile_version": profile.version,
    }


# [spec:pgorm:req:generative.artifacts]
def pins(build):
    """Interpreter, binding, toolchain and dependency pins, as installed."""
    installed = build.get("installed", {})
    capabilities = installed.get("capabilities", {})
    identity = build.get("identity", {})
    return {
        "python": installed.get("python"),
        "abi": installed.get("abi"),
        "interpreter": identity.get("interpreter"),
        "pgorm_package_version": capabilities.get("package_version"),
        "pgorm_version": capabilities.get("pgorm_version"),
        "pyo3": capabilities.get("binding"),
        "rustc": identity.get("rustc"),
        "maturin": identity.get("maturin"),
        "features": identity.get("features"),
        "capability_schema_version": capabilities.get("schema_version"),
        "target": capabilities.get("target"),
        "transport": capabilities.get("transport"),
        "dependencies": build.get("campaign_dependencies"),
        "extension_sha256": installed.get("native_sha256"),
        "wheel_sha256": build.get("wheel_sha256"),
    }


# [spec:pgorm:req:generative.artifacts]
def builds(build):
    """Build counts, so a campaign that compiled per program cannot hide it."""
    return {
        "builds_total": build.get("builds_total"),
        "builds_this_invocation": build.get("builds_this_invocation"),
        "build_seconds_this_invocation": build.get("build_seconds_this_invocation"),
    }


# [spec:pgorm:req:generative.artifacts]
def postgres(fixture):
    """Database identity and settings, taken from the fixture's own report."""
    report = getattr(fixture, "report", {}) or {}
    return {
        "image": report.get("image"),
        "image_id": report.get("image_id"),
        "workers": report.get("workers"),
        "settings": report.get("settings"),
        "baseline_sha256": report.get("baseline_sha256"),
        "state": report.get("state"),
        "cleanup_errors": report.get("cleanup_errors", []),
    }


# [spec:pgorm:req:generative.artifacts]
async def collect(root, build, profile):
    """The identity block a report carries before any work is scheduled."""
    return {
        "source": await source(root),
        "versions": versions(profile),
        "pins": pins(build),
        "builds": builds(build),
        "profile": profile.identity(),
        "class_claims": profiles.claims(),
    }


def write(artifacts, identity):
    """Retain the identity block itself as a verified artifact."""
    if not isinstance(artifacts, campaign_artifacts.Artifacts):
        raise TypeError("identity retention requires a run artifact directory")
    return artifacts.write("identity.json", identity)


__all__ = [
    "VERSION",
    "builds",
    "collect",
    "pins",
    "postgres",
    "source",
    "versions",
    "write",
]
