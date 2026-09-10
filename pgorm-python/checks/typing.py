"""Check an installed pgorm wheel's types, signatures and runnable application."""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile


def run(command, environment, *, capture=False):
    return subprocess.run(
        [str(part) for part in command],
        env=environment,
        text=True,
        capture_output=capture,
        check=True,
    )


def reject_invalid(command, source, environment):
    expected = [
        (number, match[1])
        for number, line in enumerate(source.read_text().splitlines(), 1)
        if (match := re.search(r"# expected-error: ([\w-]+)", line))
    ]
    result = subprocess.run(
        [*map(str, command), str(source)],
        env=environment,
        text=True,
        capture_output=True,
    )
    print(result.stdout, end="")
    diagnostics = re.findall(
        r"^.*?:(\d+): error: .*?\[([\w-]+)\]$", result.stdout, re.MULTILINE
    )
    actual = [(int(line), code) for line, code in diagnostics]
    if result.returncode != 1 or not expected or actual != expected:
        raise RuntimeError(
            f"expected type diagnostics {expected}, received {actual}: {result.stderr}"
        )
    return len(expected)


def installed_metadata(python, environment):
    program = """
import hashlib, importlib.metadata, json, pathlib, platform, pgorm
from pgorm import _native
root = pathlib.Path(pgorm.__file__).resolve().parent
assert (root / 'py.typed').is_file()
assert pathlib.Path(_native.__file__).parent == root
assert root == pathlib.Path(importlib.metadata.distribution('pgorm').locate_file('pgorm')).resolve()
print(json.dumps({'version': pgorm.__version__, 'python': platform.python_version(),
    'native': str(_native.__file__), 'stubs': {
        str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in sorted(root.rglob('*.pyi'))}}))
"""
    return json.loads(
        run([python, "-I", "-c", program], environment, capture=True).stdout
    )


# [spec:pgorm:req:python.typing]
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--python",
        type=Path,
        default=Path(sys.executable),
        help="interpreter with the wheel installed",
    )
    parser.add_argument("--output", type=Path, default=Path("target/python-typing"))
    options = parser.parse_args()
    if not os.environ.get("PGORM_TEST_DSN"):
        parser.error("PGORM_TEST_DSN is required; run through tests/with_postgres.py")
    root = Path(__file__).resolve().parents[2]
    output = options.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report = output / "summary.json"
    report.write_text('{"passed": false, "status": "running"}\n')
    # Keep the venv executable path: resolving its symlink selects the base interpreter.
    python = options.python.absolute()
    environment = {**os.environ, "PYTHONNOUSERSITE": "1"}
    for name in ("PYTHONPATH", "MYPYPATH"):
        environment.pop(name, None)
    metadata = installed_metadata(python, environment)
    run(
        [python, "-I", root / "pgorm-python/tests/test_signatures.py", "-v"],
        environment,
    )
    application = root / "pgorm-python/examples/application.py"
    with tempfile.TemporaryDirectory(prefix="pgorm-typing-") as temporary:
        mypy = [
            "uv",
            "tool",
            "run",
            "--from",
            "mypy==1.18.2",
            "mypy",
            "--strict",
            "--no-incremental",
            "--no-pretty",
            "--no-color-output",
            "--show-error-codes",
            "--python-executable",
            str(python),
            "--cache-dir",
            temporary,
        ]
        run([*mypy, application], environment)
        run([*mypy, "--package", "pgorm"], environment)
        rejected = reject_invalid(
            mypy, root / "pgorm-python/tests/public_types_invalid.py", environment
        )
    result = json.loads(
        run([python, "-I", application], environment, capture=True).stdout
    )
    summary = {
        "passed": True,
        "installed": metadata,
        "native_signatures": True,
        "type_checker": "mypy==1.18.2",
        "invalid_typing_cases": rejected,
        "application": result,
    }
    report.write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
