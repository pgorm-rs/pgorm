"""Build and install the private application extension once per native identity."""

import argparse
import asyncio
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import time

from . import process

ROOT = Path(__file__).resolve().parents[4]
PROBE = """
import hashlib, json, platform, sysconfig
from pathlib import Path
import pgorm as p
import pgorm._native as n
print(json.dumps({
    "native_sha256": hashlib.sha256(Path(n.__file__).read_bytes()).hexdigest(),
    "python": platform.python_version(), "abi": sysconfig.get_config_var("SOABI"),
    "capabilities": p.capabilities(),
}))
"""


def content_identity(root):
    """Include native crates, locks, registrations and the installed Python facade."""
    roots = [root / "src", root / "security/generative/bridge"]
    roots.extend(path for path in root.glob("pgorm-*") if path.is_dir())
    paths = {root / "Cargo.toml"}
    if (root / "Cargo.lock").exists():
        paths.add(root / "Cargo.lock")
    for directory in roots:
        for path in directory.rglob("*"):
            relative = path.relative_to(directory)
            if any(
                part in ("target", ".venv", "__pycache__") for part in relative.parts
            ):
                continue
            if path.is_file() and (
                path.suffix == ".rs"
                or path.name in ("Cargo.toml", "Cargo.lock", "pyproject.toml")
                or ("python" in relative.parts and path.suffix in (".py", ".pyi"))
            ):
                paths.add(path)
    digest = hashlib.sha256()
    for path in sorted(paths):
        digest.update(str(path.relative_to(root)).encode() + b"\0")
        digest.update(hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest()


def materialize(root, project):
    project.mkdir(parents=True, exist_ok=True)
    bridge = root / "security/generative/bridge"
    for name, source in (
        ("src", bridge / "src"),
        ("python", root / "pgorm-python/python"),
    ):
        target = project / name
        if target.exists():
            shutil.rmtree(target)
        shutil.copytree(
            source, target, ignore=shutil.ignore_patterns("*.pyc", "__pycache__")
        )
    manifest = (bridge / "Cargo.toml").read_text()
    manifest = manifest.replace(
        'path = "../../../pgorm-python"',
        "path = " + json.dumps(str(root / "pgorm-python")),
    ).replace('path = "../../.."', "path = " + json.dumps(str(root)))
    (project / "Cargo.toml").write_text(manifest)
    for name in ("Cargo.lock", "pyproject.toml"):
        shutil.copyfile(bridge / name, project / name)


async def probe(python):
    output = await process.run(str(python), "-I", "-c", PROBE)
    return json.loads(output.stdout)


# [spec:pgorm:req:generative.build-amortization]
async def prepare(output, *, root=ROOT, python=sys.executable):
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    source = content_identity(root)
    revision = (
        await process.run("git", "rev-parse", "HEAD", cwd=str(root))
    ).stdout.strip()
    interpreter = await process.run(
        str(python),
        "-I",
        "-c",
        "import platform,sysconfig; print(platform.python_version(),sysconfig.get_config_var('SOABI'))",
    )
    toolchain = (await process.run("rustc", "--version")).stdout.strip()
    identity = {
        "source_sha256": source,
        "revision": revision,
        "interpreter": interpreter.stdout.strip(),
        "rustc": toolchain,
        "features": ["extension-module"],
        "maturin": "1.15.0",
    }
    manifest = output / "build.json"
    installed = output / "venv/bin/python"
    previous = json.loads(manifest.read_text()) if manifest.exists() else {}
    if previous.get("identity") == identity and installed.exists():
        actual = await probe(installed)
        if actual != previous["installed"]:
            raise RuntimeError("installed extension differs from its build evidence")
        return {
            **previous,
            "builds_this_invocation": 0,
            "build_seconds_this_invocation": 0,
        }
    started = time.monotonic()
    (output / "building.json").write_text(json.dumps(identity, indent=2) + "\n")
    project = output / "project"
    materialize(root, project)
    environment = {
        **os.environ,
        "PYO3_PYTHON": str(python),
        "PYTHONNOUSERSITE": "1",
        "CARGO_TARGET_DIR": str(root / "target/generative-native"),
    }
    environment.pop("PYTHONPATH", None)
    version = await process.run(
        str(python), "-m", "maturin", "--version", environment=environment
    )
    if version.stdout.strip() != "maturin 1.15.0":
        raise RuntimeError("campaign build requires maturin 1.15.0")
    wheels = output / "dist"
    if wheels.exists():
        shutil.rmtree(wheels)
    built = await process.run(
        str(python),
        "-m",
        "maturin",
        "build",
        "--manifest-path",
        str(project / "Cargo.toml"),
        "--interpreter",
        str(python),
        "--out",
        str(wheels),
        "--locked",
        "--offline",
        environment=environment,
        timeout=900,
    )
    (output / "build.log").write_text(built.stdout + built.stderr)
    wheel_paths = list(wheels.glob("pgorm-*.whl"))
    if len(wheel_paths) != 1:
        raise RuntimeError("campaign build did not produce exactly one wheel")
    await process.run(
        "uv", "venv", "--clear", "--python", str(python), str(output / "venv")
    )
    await process.run(
        "uv",
        "pip",
        "install",
        "--no-index",
        "--no-cache",
        "--python",
        str(installed),
        str(wheel_paths[0]),
    )
    actual = await probe(installed)
    if content_identity(root) != source:
        raise RuntimeError("native source changed during compilation")
    report = {
        "identity": identity,
        "installed": actual,
        "python": str(installed),
        "wheel": str(wheel_paths[0]),
        "wheel_sha256": hashlib.sha256(wheel_paths[0].read_bytes()).hexdigest(),
        "builds_total": previous.get("builds_total", 0) + 1,
        "builds_this_invocation": 1,
        "build_seconds_this_invocation": time.monotonic() - started,
    }
    manifest.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    (output / "building.json").unlink()
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target/generative-build")
    arguments = parser.parse_args()
    report = asyncio.run(prepare(arguments.output))
    print(
        json.dumps(
            {
                key: report[key]
                for key in (
                    "python",
                    "builds_total",
                    "builds_this_invocation",
                    "build_seconds_this_invocation",
                )
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
