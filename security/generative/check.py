#!/usr/bin/env python3
"""Run the fast campaign regressions without Docker, a database or a campaign.

These are the tests that demonstrate the runner fails: empty discovery, dead
dispatch, undetected controls, weakened oracles, replay divergence, deadlines,
malformed artifacts and cleanup failure. They own no PostgreSQL fixture and
build no native extension, so they belong in front of every campaign and in
front of every commit. The campaigns themselves stay out of both.
"""

import os
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def _cargo(manifest, *extra):
    return [
        "cargo",
        "test",
        "--locked",
        "--manifest-path",
        str(HERE / manifest),
        "--target-dir",
        str(ROOT / "target"),
        *extra,
    ]


# [spec:pgorm:req:generative.ci]
def main():
    commands = [
        [
            sys.executable,
            "-m",
            "unittest",
            "discover",
            "-s",
            str(HERE / "tests"),
            "-p",
            "test_*.py",
        ],
        # The replay harness is the Rust half of the parity claim, and the
        # codegen driver is the compile suite's; a campaign that cannot replay
        # or compile its own findings has nothing to report.
        _cargo("replay/Cargo.toml", "--lib"),
        _cargo("compile/Cargo.toml"),
    ]
    environment = {**os.environ, "PYTHONPATH": str(HERE / "src")}
    failed = False
    for command in commands:
        result = subprocess.run(command, cwd=ROOT, check=False, env=environment)
        failed |= result.returncode != 0
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
