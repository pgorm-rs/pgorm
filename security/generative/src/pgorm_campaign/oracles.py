"""Account for every observation and compare independently executed fixture state."""

import asyncio
import time

from . import comparison
from .reference import Driver, Reference
from .reference_inspection import inspect_reads
from .reference_state import snapshot


def compare(
    program, subject, reference, subject_state, reference_state, *, control=None
):
    if control is not None and subject.get("control") != {
        "id": control,
        "dispatched": True,
    }:
        raise comparison.InvalidOracle("independent control has no dispatch evidence")
    expected = [step["id"] for step in program["steps"]]
    for name, report in (("subject", subject), ("reference", reference)):
        if report.get("status") not in ("executed", "error") or report.get(
            "cleanup_errors"
        ):
            raise comparison.InvalidOracle(name + " execution or cleanup is incomplete")
        if [step.get("id") for step in report.get("steps", [])] != expected:
            raise comparison.InvalidOracle(
                name + " observations are missing or reordered"
            )
    policies = {
        observation["step"]: observation for observation in program["observations"]
    }
    if subject.get("unconstructed") and any(
        policy["oracle"] != "exact-error"
        for key, policy in policies.items()
        if key != "final"
    ):
        raise comparison.InvalidOracle(
            "subject left scheduled operations unconstructed"
        )
    results = []
    for step, actual, independent in zip(
        program["steps"], subject["steps"], reference["steps"], strict=True
    ):
        policy = policies[step["id"]]
        if actual.get("status") == "observed":
            if control is None and not actual.get("native_paths"):
                raise comparison.InvalidOracle(
                    "subject observation has no native execution evidence"
                )
            if control is not None and actual.get("native_paths"):
                raise comparison.InvalidOracle(
                    "independent control claims native execution"
                )
        elif actual.get("status") != "error":
            raise comparison.InvalidOracle(
                "subject observation is not a completed effect"
            )
        actual = actual.get("inspection_probe", actual["observation"])
        independent = independent["observation"]
        if policy["oracle"] == "exact-error":
            outcome = comparison.exact_error(actual, policy["error"])
            other = comparison.exact_error(independent, policy["error"])
            if not other.equal:
                raise comparison.InvalidOracle(
                    "reference did not establish the declared rejection: " + step["id"]
                )
        elif policy["oracle"] == "reference":
            if independent["kind"] == "error":
                raise comparison.InvalidOracle(
                    "reference rejected a program without an exact-error declaration"
                )
            outcome = comparison.observation(
                actual, independent, ordered=step["data"].get("ordered", False)
            )
        else:
            raise comparison.InvalidOracle(
                "oracle semantics uncovered: " + policy["oracle"]
            )
        results.append(
            {
                "step": step["id"],
                "oracle": policy["oracle"],
                "equal": outcome.equal,
                "reason": outcome.reason,
            }
        )
    if subject_state is None or reference_state is None:
        raise comparison.InvalidOracle("missing final fixture state")
    if policies.get("final", {}).get("oracle") != "fixture-state":
        raise comparison.InvalidOracle("missing fixture-state policy")
    equal = comparison.encoded(subject_state) == comparison.encoded(reference_state)
    results.append(
        {
            "step": "final",
            "oracle": "fixture-state",
            "equal": equal,
            "reason": "equal" if equal else "final fixture state differs",
        }
    )
    return results


# [spec:pgorm:req:generative.oracles]
class Checker:
    def __init__(self, executor):
        self.executor = executor
        self.lock = asyncio.Lock()

    async def run(self, program, *, timeout=10):
        async with self.lock:
            started = time.monotonic()
            report = {
                "program_sha256": program.digest,
                "status": "incomplete",
                "comparisons": [],
                "cleanup_errors": [],
            }
            try:
                report["subject"] = await self.executor.run(program, timeout=timeout)
                async with asyncio.timeout(timeout):
                    await inspect_reads(self.executor, program, report["subject"])
                    async with await Driver.connect(
                        self.executor.fixture, worker=self.executor.worker
                    ) as reference:
                        report["reference"] = await Reference(reference).run(
                            program, timeout=timeout
                        )
                        report["reference_state"] = await snapshot(reference)
                    async with await Driver.connect(
                        self.executor.fixture,
                        worker=self.executor.worker,
                        side="subject",
                    ) as subject:
                        report["subject_state"] = await snapshot(subject)
                report["comparisons"] = compare(
                    program.data(),
                    report["subject"],
                    report["reference"],
                    report["subject_state"],
                    report["reference_state"],
                )
                if any(
                    report[side].get("program_sha256") != program.digest
                    for side in ("subject", "reference")
                ):
                    raise comparison.InvalidOracle(
                        "observations belong to a different program"
                    )
                if not all(item["equal"] for item in report["comparisons"]):
                    report["status"] = "defect"
                elif any(
                    item["oracle"] == "exact-error" for item in report["comparisons"]
                ):
                    report["status"] = "expected-rejection"
                else:
                    report["status"] = "pass"
            except Exception as error:
                report["error"] = {"class": type(error).__name__, "cause": str(error)}
            finally:
                report["seconds"] = time.monotonic() - started
            return report
