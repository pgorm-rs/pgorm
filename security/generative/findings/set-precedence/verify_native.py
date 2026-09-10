"""Execute SQL emitted by standalone Rust in an owned PostgreSQL fixture."""

import asyncio
import hashlib
import json
from pathlib import Path
import uuid

from pgorm_campaign import baseline, comparison, process
from pgorm_campaign.build import content_identity, ROOT
from pgorm_campaign.fixtures import Fixture
from pgorm_campaign.reference import Driver
from pgorm_campaign.reference_sql import SQL


async def main():
    source_identity = content_identity(ROOT)
    root = Path("target/generative-native-findings")
    output = root / ("run-" + uuid.uuid4().hex[:12])
    output.mkdir(parents=True)
    (root / "summary.json").write_text(
        json.dumps({"passed": False, "output": str(output)}) + "\n"
    )
    compiled = await process.run(
        "cargo",
        "run",
        "--locked",
        "--offline",
        "--quiet",
        "--manifest-path",
        "security/generative/findings/set-precedence/Cargo.toml",
        "--target-dir",
        "target",
        timeout=120,
    )
    (output / "native-build.log").write_text(compiled.stderr)
    native = json.loads(compiled.stdout)
    if (
        not isinstance(native, list)
        or len(native) != 3
        or not all(isinstance(sql, str) for sql in native)
    ):
        raise RuntimeError("native renderer omitted the expected queries")
    simple = "SELECT id FROM fixture.accounts"
    appended = "(" + simple + ") UNION ALL (" + simple + ")"
    intersected = "(" + appended + ") INTERSECT ALL (" + simple + ")"
    reference = [
        appended,
        intersected,
        "(" + intersected + ") EXCEPT ALL (" + simple + ")",
    ]
    results = []
    async with Fixture(output / "fixture") as fixture:
        await fixture.reset(0, baseline.default(), rebuild=True)
        async with await Driver.connect(fixture, side="subject") as subject:
            async with await Driver.connect(fixture) as independent:
                for label, actual, expected in zip(
                    ("append", "intersect", "remove"), native, reference, strict=True
                ):
                    rows, _ = await subject.query(SQL((actual,)))
                    wanted, _ = await independent.query(SQL((expected,)))
                    result = comparison.rows(rows, wanted, ordered=False)
                    results.append(
                        {
                            "operation": label,
                            "native_sql": actual,
                            "reference_sql": expected,
                            "native_rows": rows,
                            "reference_rows": wanted,
                            "equal": result.equal,
                            "reason": result.reason,
                        }
                    )
    # Detection evidence for the open native-renderer finding, not a clean program.
    if content_identity(ROOT) != source_identity:
        raise RuntimeError("native source changed during diagnostic execution")
    passed = [item["equal"] for item in results] == [True, False, False]
    report = {
        "passed": passed,
        "output": str(output),
        "scope": "standalone-rust-renderer-with-independent-postgresql-driver",
        "source_sha256": source_identity,
        "native_executable_sha256": hashlib.sha256(
            Path("target/debug/pgorm-set-replay").read_bytes()
        ).hexdigest(),
        "native_lock_sha256": hashlib.sha256(
            Path("security/generative/findings/set-precedence/Cargo.lock").read_bytes()
        ).hexdigest(),
        "results": results,
    }
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    (root / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(
        json.dumps(
            {
                "passed": passed,
                "output": str(output),
                "row_counts": [
                    [len(item[side]) for side in ("native_rows", "reference_rows")]
                    for item in results
                ],
            }
        )
    )
    if not passed:
        raise SystemExit(1)


if __name__ == "__main__":
    asyncio.run(main())
