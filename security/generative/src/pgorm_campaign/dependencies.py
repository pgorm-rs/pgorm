"""Prepare pinned oracle dependencies separately from the native build cache."""

import json
import tomllib

from . import process

PIN = "psycopg[binary]==3.3.5"
VERSIONS = {"psycopg": "3.3.5", "psycopg-binary": "3.3.5"}
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


async def prepare(python, root):
    project = tomllib.loads((root / "security/generative/pyproject.toml").read_text())
    if project["project"]["dependencies"] != [PIN]:
        raise RuntimeError(
            "campaign dependency declarations and installation pins disagree"
        )
    result = json.loads((await process.run(str(python), "-I", "-c", PROBE)).stdout)
    changed = any(
        result[name] is None or result[name]["version"] != version
        for name, version in VERSIONS.items()
    )
    if changed:
        await process.run(
            "uv",
            "pip",
            "install",
            "--only-binary=:all:",
            "--python",
            str(python),
            PIN,
            timeout=180,
        )
        result = json.loads((await process.run(str(python), "-I", "-c", PROBE)).stdout)
    if any(
        result[name] is None or result[name]["version"] != version
        for name, version in VERSIONS.items()
    ):
        raise RuntimeError("installed oracle dependencies do not match pinned versions")
    return {"pin": PIN, "installed": result, "installed_this_invocation": changed}
