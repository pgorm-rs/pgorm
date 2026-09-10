"""Complete sensitivity checks using an owned disposable PostgreSQL fixture."""

import asyncio
import json
from pathlib import Path
import uuid
import zipfile

from pgorm_campaign.control_catalog import catalog, VERSION
from pgorm_campaign.control_verdict import summary
from pgorm_campaign.controls import Controls
from pgorm_campaign.executor import Executor
from pgorm_campaign.fixtures import Fixture


# [spec:pgorm:req:generative.controls/test]
async def main():
    root = Path("target/generative-controls")
    output = root / ("run-" + uuid.uuid4().hex[:12])
    output.mkdir(parents=True)
    (root / "summary.json").write_text(
        json.dumps({"passed": False, "output": str(output)}) + "\n"
    )
    build = json.loads(Path("target/generative-build/build.json").read_text())
    with zipfile.ZipFile(build["wheel"]) as wheel:
        if any(
            name.startswith(("pgorm_campaign/", "security/"))
            for name in wheel.namelist()
        ):
            raise RuntimeError(
                "test-only campaign controls leaked into the public wheel"
            )
    (output / "build.json").write_text(json.dumps(build, indent=2) + "\n")
    specs, reports = catalog(), []
    async with Fixture(output / "fixture") as fixture:
        async with Executor(
            fixture, expected_native_sha256=build["installed"]["native_sha256"]
        ) as executor:
            controls = Controls(executor)
            for spec in specs:
                report = await controls.run(spec)
                reports.append(report)
                (output / (spec.id + ".program.json")).write_text(
                    spec.program.encoded + "\n"
                )
                (output / (spec.id + ".json")).write_text(
                    json.dumps(report, indent=2) + "\n"
                )
                print(
                    json.dumps(
                        {
                            key: report.get(key)
                            for key in ("id", "status", "reason", "error")
                        }
                    ),
                    flush=True,
                )
    result = {
        **summary(specs, reports),
        "version": VERSION,
        "output": str(output),
        "public_wheel_excludes_controls": True,
    }
    (root / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    if not result["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    asyncio.run(main())
