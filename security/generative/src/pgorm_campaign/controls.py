"""Run deliberate mistakes independently, after real pgorm baselines pass."""

import asyncio
import time

from . import control_mutations as mutations, control_verdict as verdict, oracles
from .comparison import InvalidOracle
from .reference import Driver, Reference
from .reference_state import snapshot


# [spec:pgorm:req:generative.controls]
class Controls:
    def __init__(self, executor):
        self.executor = executor
        self.checker = oracles.Checker(executor)
        self.lock = asyncio.Lock()

    async def run(self, spec, *, timeout=30):
        async with self.lock:
            started = time.monotonic()
            report = {
                "id": spec.id,
                "family": spec.family,
                "program_sha256": spec.program.digest,
                "status": "incomplete",
                "backend": spec.backend,
                "dispatch_count": 0,
                "comparisons": [],
            }
            try:
                async with asyncio.timeout(timeout):
                    report["baseline"] = await self.checker.run(spec.program)
                    verdict.baseline(spec, report["baseline"])
                    await self.attempt(spec, report)
                    report["comparisons"] = oracles.compare(
                        spec.program.data(),
                        report["control"],
                        report["reference"],
                        report["subject_state"],
                        report["reference_state"],
                        control=spec.id,
                    )
                    report["status"], report["reason"] = verdict.assess(spec, report)
            except InvalidOracle as error:
                report.update(status="invalid-control", reason=str(error))
            except Exception as error:
                report["error"] = {"class": type(error).__name__, "cause": str(error)}
            finally:
                report["seconds"] = time.monotonic() - started
            return report

    async def attempt(self, spec, report):
        original = report["baseline"]
        subject = verdict.subject(original["subject"], spec.id)
        if spec.backend == "observation":
            report["control"] = mutations.observation(subject, spec.id)
            report["dispatch_count"] += 1
            report["reference"] = original["reference"]
            report["subject_state"] = original["subject_state"]
            report["reference_state"] = original["reference_state"]
        elif spec.backend == "postgres":
            await self.executor.reset(spec.program.data())
            async with await Driver.connect(
                self.executor.fixture, worker=self.executor.worker
            ) as reference:
                report["reference"] = await Reference(reference).run(spec.program)
                report["reference_state"] = await snapshot(reference)
            async with await Driver.connect(
                self.executor.fixture, worker=self.executor.worker, side="subject"
            ) as driver:
                report["control"], report["commands"] = await mutations.postgres(
                    driver, subject, spec.id
                )
                report["dispatch_count"] += 1
                report["subject_state"] = await snapshot(driver)
        else:
            raise InvalidOracle("unknown control backend")
