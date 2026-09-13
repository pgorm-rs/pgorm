"""Execute a scheduled campaign and give every item exactly one verdict.

The runner is deliberately ignorant of how its collaborators were built. It is
handed a checker per worker, a control driver per worker, an optional compile
suite and an optional cleanup step, which is what lets the failure paths this
must refuse — a skipped item, an artifact that will not read back, a cleanup
that raises — be exercised without a database.

Work is driven off the pre-built schedule rather than off a generator, so an
item that never ran leaves a hole that `campaign_verdict.work_faults` finds by
comparing two independently produced lists.
"""

import asyncio
from pathlib import Path
import time

from . import (
    campaign_coverage,
    campaign_failure,
    campaign_identity,
    campaign_report,
    campaign_verdict,
    control_verdict,
    grammar,
    matrix,
    profiles,
)
from .control_catalog import catalog
from .grammar_state import structure

# Errors whose presence means the item ran out of time rather than disagreeing.
DEADLINES = ("TimeoutError", "CancelledError")


class RunnerError(RuntimeError):
    """The runner was given collaborators it cannot honestly run work with."""


def _paths(report):
    """Every native API path the subject recorded for one program."""
    subject = report.get("subject") or {}
    found = set()
    for step in subject.get("steps", ()):
        found.update(step.get("native_paths") or ())
    for event in subject.get("trace", ()):
        found.update(event.get("native_paths") or ())
    return sorted(found)


def _declared(program):
    """What the program itself says its outcome should be."""
    return (
        "expected-rejection"
        if any(
            item["oracle"] == "exact-error" for item in program.data()["observations"]
        )
        else "pass"
    )


def _timed_out(report):
    error = report.get("error") or {}
    return error.get("class") in DEADLINES


def _cleanup_errors(report):
    errors = list(report.get("cleanup_errors") or ())
    subject = report.get("subject") or {}
    errors.extend(subject.get("cleanup_errors") or ())
    return [str(item) for item in errors]


# [spec:pgorm:req:generative.verdict]
class Runner:
    """Drive one campaign profile to a complete, fault-checked report."""

    def __init__(
        self,
        profile,
        plan,
        artifacts,
        *,
        checkers,
        controls=None,
        control_specs=None,
        compile_suite=None,
        cleanup=None,
        identity=None,
        postgres=None,
        fixture=None,
        generate=grammar.generate,
        root=Path("."),
    ):
        self.profile = profile
        self.plan = plan
        self.artifacts = artifacts
        self.checkers = dict(checkers or {})
        self.controls = dict(controls or {})
        self.control_specs = catalog() if control_specs is None else list(control_specs)
        self.compile_suite = compile_suite
        self.cleanup = cleanup
        self.identity = identity or {}
        self.postgres = postgres or {}
        self.fixture = fixture
        self.generate = generate
        self.root = Path(root)
        self.records = []
        self.tokens = set()
        self.timings = {}
        self.cleanup_errors = []
        self.registry = matrix.load()
        self.control_reports = {}

    def _limits(self):
        return self.profile.grammar_limits()

    def _checker(self, worker):
        checker = self.checkers.get(worker)
        if checker is None:
            raise RunnerError("no checker is available for worker " + str(worker))
        return checker

    def _record(self, item, **fields):
        record = item.data()
        record.update(fields)
        self.records.append(record)
        self.tokens.add("class." + item.run_class)
        return record

    async def _construction(self):
        """Build programs and count them as built. Nothing here reaches a database."""
        items = self.plan.by_class("construction")
        if not items:
            return
        started = time.monotonic()
        inventory = []
        for item in items:
            began = time.monotonic()
            outcome, detail = self._construct(item)
            self._record(
                item,
                verdict=campaign_verdict.from_construction(outcome),
                expected="pass",
                seconds=time.monotonic() - began,
                error=outcome.get("error"),
                program_sha256=outcome.get("program_sha256"),
                reached_database=False,
                oracle_decided=False,
            )
            inventory.append(detail)
        elapsed = time.monotonic() - started
        self.artifacts.write(
            "construction/programs.json",
            {
                "claim": (
                    "constructor calls only; none of these programs opened a "
                    "database connection or was decided by an oracle"
                ),
                "construction_only_programs_constructed": sum(
                    1 for entry in inventory if "error" not in entry
                ),
                "programs_per_second": len(inventory) / elapsed if elapsed else None,
                "programs": inventory,
                "seconds": elapsed,
            },
        )
        self.timings["construction"] = elapsed

    def _construct(self, item):
        """One constructor call, round-tripped through the portable format."""
        try:
            generated = self.generate(
                item.seed,
                item.index,
                family=item.family,
                mode=item.mode,
                limits=self._limits(),
            )
            data = generated.program.data()
            detail = {
                "id": item.id,
                "seed": item.seed,
                "index": item.index,
                "family": item.family,
                "mode": item.mode,
                "program_sha256": generated.program.digest,
                "structure_sha256": structure(generated.program),
                "nodes": len(data["nodes"]),
                "steps": len(data["steps"]),
            }
            return {
                "constructed": True,
                "program_sha256": generated.program.digest,
            }, detail
        except Exception as error:
            detail = {
                "id": item.id,
                "seed": item.seed,
                "index": item.index,
                "family": item.family,
                "mode": item.mode,
                "error": type(error).__name__ + ": " + str(error),
            }
            return {
                "constructed": False,
                "error": {"class": type(error).__name__, "cause": str(error)},
            }, detail

    async def _live(self, name):
        """Run one live class, one sequential stream of work per worker."""
        items = self.plan.by_class(name)
        if not items:
            return
        started = time.monotonic()
        workers = sorted({item.worker for item in items})
        await asyncio.gather(
            *(
                self._worker_stream(
                    name, worker, [i for i in items if i.worker == worker]
                )
                for worker in workers
            )
        )
        self.timings[name] = time.monotonic() - started

    async def _worker_stream(self, name, worker, items):
        checker = self._checker(worker)
        for item in items:
            await self._program_item(name, item, checker)

    async def _program_item(self, name, item, checker):
        """Generate, execute and decide one program, retaining what it produced."""
        began = time.monotonic()
        try:
            generated = self.generate(
                item.seed,
                item.index,
                family=item.family,
                mode=item.mode,
                limits=self._limits(),
            )
        except Exception as error:
            self._record(
                item,
                verdict="incomplete",
                seconds=time.monotonic() - began,
                error={"class": type(error).__name__, "cause": str(error)},
            )
            return
        program = generated.program
        prefix = name + "/" + item.id
        entries = [
            self.artifacts.text(prefix + ".program.json", program.encoded + "\n"),
            self.artifacts.write(prefix + ".recipe.json", generated.recipe()),
        ]
        report = await checker.run(
            program, timeout=self.profile.budgets["program_timeout_seconds"]
        )
        entries.append(self.artifacts.write(prefix + ".oracle.json", report))
        verdict = campaign_verdict.from_checker(report)
        self.tokens |= campaign_coverage.observed(
            program.data(), report, registry=self.registry
        )
        if verdict in ("pass", "defect", "expected-rejection"):
            self.tokens.add("family." + str(item.family))
        record = self._record(
            item,
            verdict=verdict,
            expected=_declared(program),
            seconds=time.monotonic() - began,
            program_sha256=program.digest,
            error=report.get("error"),
            native_paths=_paths(report),
            cleanup_errors=_cleanup_errors(report),
            deadline_exceeded=_timed_out(report),
            builds=(report.get("subject") or {}).get("builds", 0),
            comparisons=report.get("comparisons", []),
            artifacts=[entry["name"] for entry in entries],
            artifact_faults=[
                {"kind": "artifact-malformed", "detail": entry["name"]}
                for entry in entries
                if entry.get("sha256") is None
            ],
        )
        if not campaign_verdict.accepted(record):
            finding = await self._retain(item, record, program, report, checker)
            record["finding"] = finding
            record["artifact_faults"].extend(finding.get("artifact_faults") or ())
            if finding.get("retained") is False:
                record["artifact_faults"].append(
                    {
                        "kind": "artifact-missing",
                        "detail": "failure retention: " + str(finding.get("reason")),
                    }
                )

    async def _retain(self, item, record, program, report, checker):
        try:
            return await campaign_failure.retain(
                self.artifacts.directory / "findings" / item.id,
                record,
                program,
                report,
                self._attribution_identity(),
                checker=checker,
                budget=self.profile.shrink_budget(),
                root=self.root,
            )
        except Exception as error:
            return {
                "retained": False,
                "reason": type(error).__name__ + ": " + str(error),
            }

    def _attribution_identity(self):
        source = self.identity.get("source", {})
        return {
            "source_sha256": source.get("content_sha256"),
            "revision": source.get("revision"),
            "extension_sha256": self.identity.get("pins", {}).get("extension_sha256"),
        }

    async def _controls(self):
        """Every declared control runs, and each one is accounted separately."""
        items = self.plan.by_class("control")
        if not items:
            return
        started = time.monotonic()
        specs = {spec.id: spec for spec in self.control_specs}
        workers = sorted({item.worker for item in items})
        await asyncio.gather(
            *(
                self._control_stream(
                    worker, [i for i in items if i.worker == worker], specs
                )
                for worker in workers
            )
        )
        self.timings["control"] = time.monotonic() - started

    async def _control_stream(self, worker, items, specs):
        driver = self.controls.get(worker)
        if driver is None:
            raise RunnerError("no control driver for worker " + str(worker))
        for item in items:
            await self._control_item(item, specs.get(item.control_id), driver)

    async def _control_item(self, item, spec, driver):
        began = time.monotonic()
        if spec is None:
            self._record(
                item,
                verdict="invalid-control",
                seconds=time.monotonic() - began,
                error={"class": "MissingControl", "cause": str(item.control_id)},
            )
            return
        report = await driver.run(
            spec, timeout=self.profile.budgets["control_timeout_seconds"]
        )
        self.control_reports[item.control_id] = report
        entries = [
            self.artifacts.text(
                "control/" + item.control_id + ".program.json",
                spec.program.encoded + "\n",
            ),
            self.artifacts.write("control/" + item.control_id + ".json", report),
        ]
        baseline = report.get("baseline") or {}
        self.tokens |= campaign_coverage.observed(
            spec.program.data(), baseline, registry=self.registry
        )
        self._record(
            item,
            verdict=campaign_verdict.from_control(report),
            seconds=time.monotonic() - began,
            program_sha256=spec.program.digest,
            error=report.get("error"),
            native_paths=_paths(baseline),
            cleanup_errors=_cleanup_errors(baseline),
            deadline_exceeded=_timed_out(report),
            artifacts=[entry["name"] for entry in entries],
            artifact_faults=[
                {"kind": "artifact-malformed", "detail": entry["name"]}
                for entry in entries
                if entry.get("sha256") is None
            ],
        )

    async def _compile(self):
        """One scheduled item whose evidence keeps its own compile vocabulary."""
        items = self.plan.by_class("compile")
        if not items:
            return None
        item = items[0]
        began = time.monotonic()
        document = None
        error = None
        if self.compile_suite is None:
            error = {
                "class": "MissingCompileSuite",
                "cause": "no compile suite supplied",
            }
        else:
            try:
                document = await self.compile_suite()
            except Exception as failure:
                error = {"class": type(failure).__name__, "cause": str(failure)}
        entry = self.artifacts.write("compile/compile-report.json", document or {})
        self._record(
            item,
            verdict=campaign_verdict.from_compile(document),
            seconds=time.monotonic() - began,
            error=error,
            artifacts=[entry["name"]],
            artifact_faults=[]
            if entry.get("sha256")
            else [{"kind": "artifact-malformed", "detail": entry["name"]}],
        )
        self.timings["compile"] = time.monotonic() - began
        return document

    async def _close(self):
        if self.cleanup is None:
            return
        try:
            errors = await self.cleanup()
        except Exception as error:
            self.cleanup_errors.append(type(error).__name__ + ": " + str(error))
            return
        self.cleanup_errors.extend(str(item) for item in errors or ())

    def _control_summary(self):
        if not self.plan.by_class("control"):
            return {"passed": False, "reason": "no controls were scheduled"}
        ordered = [
            spec for spec in self.control_specs if spec.id in self.control_reports
        ]
        reports = [self.control_reports[spec.id] for spec in ordered]
        result = control_verdict.summary(self.control_specs, reports)
        result["reason"] = (
            "all declared controls detected their intended defect"
            if result["passed"]
            else "controls did not all detect their intended defect"
        )
        return result

    def _faults(self):
        faults = campaign_verdict.work_faults(self.plan, self.records)
        faults.extend(campaign_verdict.worker_faults(self.profile, self.records))
        faults.extend(
            campaign_verdict.budget_faults(
                self.profile,
                {
                    "class_seconds": {
                        name: self.timings[name]
                        for name in profiles.CLASSES
                        if name in self.timings
                    },
                    "total_seconds": self.timings.get("total_seconds", 0),
                },
            )
        )
        for message in self.cleanup_errors:
            faults.append(campaign_verdict.fault("cleanup-failure", message))
        for entry in self.artifacts.recheck():
            faults.append(campaign_verdict.fault(entry["kind"], entry["detail"]))
        return faults

    # [spec:pgorm:req:generative.verdict]
    # [spec:pgorm:req:generative.artifacts]
    async def run(self):
        """Execute the whole schedule and assemble the retained run document."""
        started = time.monotonic()
        compile_document = None
        try:
            await self._construction()
            await self._live("runtime")
            await self._live("invalid")
            await self._controls()
            compile_document = await self._compile()
        finally:
            await self._close()
            self.timings["total_seconds"] = time.monotonic() - started
        coverage = campaign_coverage.assess(self.profile, self.plan, self.tokens)
        controls = self._control_summary()
        faults = self._faults()
        aggregate = campaign_verdict.aggregate(
            self.profile,
            self.plan,
            self.records,
            coverage=coverage,
            controls=controls,
            faults=faults,
        )
        document = campaign_report.assemble(
            self.profile,
            self.plan,
            self.records,
            identity=self.identity,
            postgres=campaign_identity.postgres(self.fixture)
            if self.fixture is not None
            else self.postgres,
            coverage=coverage,
            controls=controls,
            compile_document=compile_document,
            verdicts=campaign_verdict.counts(self.records),
            timing=self.timings,
            aggregate=aggregate,
            artifacts=self.artifacts.manifest(),
        )
        self.artifacts.write("campaign.json", document)
        campaign_identity.write(self.artifacts, self.identity)
        self.artifacts.write("plan.json", self.plan.data())
        return document


__all__ = ["Runner", "RunnerError"]
