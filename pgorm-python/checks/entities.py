"""Build an independent application registration wheel and test its public API."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def run(command, environment):
    subprocess.run(command, env=environment, check=True)


def materialize(root, destination):
    fixture = root / "pgorm-python/tests/application-binding"
    shutil.copytree(fixture / "src", destination / "src")
    shutil.copytree(root / "pgorm-python/python", destination / "python",
                    ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    manifest = (fixture / "Cargo.toml").read_text()
    manifest = manifest.replace('path = "../.."', "path = " + json.dumps(str(root / "pgorm-python")))
    manifest = manifest.replace('path = "../../.."', "path = " + json.dumps(str(root)))
    (destination / "Cargo.toml").write_text(manifest)
    shutil.copyfile(fixture / "Cargo.lock", destination / "Cargo.lock")
    shutil.copyfile(fixture / "pyproject.toml", destination / "pyproject.toml")


# [spec:pgorm:req:python.entities/test]
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("target/python-entities"))
    options = parser.parse_args()
    if not os.environ.get("PGORM_TEST_DSN"):
        parser.error("PGORM_TEST_DSN is required; run through tests/with_postgres.py")
    root = Path(__file__).resolve().parents[2]
    output = options.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    (output / "summary.json").write_text('{"passed": false, "status": "running"}\n')
    environment = {**os.environ, "PYO3_PYTHON": sys.executable, "CARGO_TARGET_DIR": str(root / "target"), "PYTHONNOUSERSITE": "1"}
    environment.pop("PYTHONPATH", None)
    with tempfile.TemporaryDirectory(prefix="pgorm-entity-project-") as directory:
        project = Path(directory)
        materialize(root, project)
        run(["cargo", "test", "--manifest-path", str(project / "Cargo.toml"), "--locked", "--lib"], environment)
        run([sys.executable, "-m", "maturin", "build", "--manifest-path", str(project / "Cargo.toml"),
             "--interpreter", sys.executable, "--out", str(project / "dist"), "--locked"], environment)
        wheels = list((project / "dist").glob("pgorm-*.whl"))
        if len(wheels) != 1:
            raise RuntimeError("expected one downstream application wheel")
        wheel = output / wheels[0].name
        shutil.copyfile(wheels[0], wheel)
        run(["uv", "venv", "--python", sys.executable, str(project / "venv")], environment)
        python = project / "venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        run(["uv", "pip", "install", "--no-index", "--no-cache", "--python", str(python), str(wheel)], environment)
        run([str(python), "-I", str(root / "pgorm-python/tests/registered_entities.py"), "-v"], environment)
    (output / "summary.json").write_text(json.dumps({"passed": True, "registered_entities": ["app.Account", "app.Note"], "wheel": wheel.name}, indent=2) + "\n")


if __name__ == "__main__":
    main()
