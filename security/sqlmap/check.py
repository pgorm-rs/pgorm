#!/usr/bin/env python3
"""Run the fast sqlmap runner regressions without Docker or a live scanner."""
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent


# [spec:pgorm:req:security.sqlmap.runner-tests]
def main():
    commands = [
        [sys.executable, "-m", "unittest", "discover", "-s", str(HERE), "-p", "test_*.py"],
        ["cargo", "test", "--locked", "--manifest-path", str(HERE / "adapter/Cargo.toml"), "--test", "runner"],
    ]
    failed = False
    for command in commands:
        failed |= subprocess.run(command, cwd=HERE.parent.parent, check=False).returncode != 0
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
