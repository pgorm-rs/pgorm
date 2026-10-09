"""Prepare oracle dependencies separately from the native build cache."""

import json
import tomllib

from . import process

REQUIREMENT = "psycopg[binary]>=3.3,<4"
NAMES = ("psycopg", "psycopg-binary")
PROBE = """
import hashlib, importlib.metadata as metadata, json
result = {}
for name in ('psycopg', 'psycopg-binary'):
    try:
        distribution = metadata.distribution(name)
    except metadata.PackageNotFoundError:
        result[name] = None
    else:
        result[name] = {
            'version': distribution.version,
            'record_sha256': hashlib.sha256(distribution.read_text('RECORD').encode()).hexdigest(),
        }
print(json.dumps(result))
"""


def satisfied(result):
    """Both distributions present, one release of each, within REQUIREMENT."""
    if any(result[name] is None for name in NAMES):
        return False
    versions = {result[name]["version"] for name in NAMES}
    if len(versions) != 1:
        return False
    major, minor = (int(part) for part in versions.pop().split(".")[:2])
    return major == 3 and minor >= 3


async def prepare(python, root):
    project = tomllib.loads((root / "security/generative/pyproject.toml").read_text())
    if project["project"]["dependencies"] != [REQUIREMENT]:
        raise RuntimeError(
            "campaign dependency declarations and installation requirement disagree"
        )
    result = json.loads((await process.run(str(python), "-I", "-c", PROBE)).stdout)
    changed = not satisfied(result)
    if changed:
        await process.run(
            "uv",
            "pip",
            "install",
            "--only-binary=:all:",
            "--python",
            str(python),
            REQUIREMENT,
            timeout=180,
        )
        result = json.loads((await process.run(str(python), "-I", "-c", PROBE)).stdout)
    if not satisfied(result):
        raise RuntimeError(
            "installed oracle dependencies do not satisfy " + REQUIREMENT
        )
    return {
        "requirement": REQUIREMENT,
        "installed": result,
        "installed_this_invocation": changed,
    }
