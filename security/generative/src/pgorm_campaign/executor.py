"""Reuse public native pools and one event loop across portable programs."""

import asyncio
import hashlib
from pathlib import Path
import time

from . import baseline, observations
from .effects import Effects
from .program import Program
from .resolution import Resolution
from .runtime_guard import forbid_processes


# [spec:pgorm:req:generative.execution]
# [spec:pgorm:req:generative.build-amortization]
class Executor:
    def __init__(self, fixture, *, worker=0, expected_native_sha256=None):
        import pgorm as p
        import pgorm._native as native

        self.p = p
        self.fixture = fixture
        self.worker = worker
        self.loop = asyncio.get_running_loop()
        self.native_file = Path(native.__file__)
        self.native_sha256 = hashlib.sha256(self.native_file.read_bytes()).hexdigest()
        if (
            expected_native_sha256 is not None
            and self.native_sha256 != expected_native_sha256
        ):
            raise RuntimeError(
                "loaded extension does not match campaign build identity"
            )
        self.capabilities = p.capabilities()
        self.subject = None
        self.reference = None
        self.fixture_sha256 = baseline.digest(baseline.default())
        self.rebuild = False
        self.programs = 0
        self.closed = False
        self.lock = asyncio.Lock()

    async def reset(self, program):
        fixture = program["fixture"]
        changed = baseline.digest(fixture) != self.fixture_sha256
        ddl = any(node["op"].startswith("schema.") for node in program["nodes"])
        rebuild = self.rebuild or changed or ddl
        if rebuild:
            await self.close_pools()
        await self.fixture.reset(self.worker, fixture, rebuild=rebuild)
        self.fixture_sha256 = baseline.digest(fixture)
        self.rebuild = ddl
        if self.subject is None:
            pair = self.fixture.pair(self.worker)
            options = {
                "max_size": 2,
                "connect_timeout": 5,
                "acquire_timeout": 5,
                "statement_cache_size": 64,
            }
            self.subject = self.p.Pool(pair.subject, **options)
            self.reference = self.p.Pool(pair.reference, **options)

    async def run(self, program, *, timeout=10):
        if not isinstance(program, Program):
            raise TypeError("executor requires a validated Program")
        if self.closed or asyncio.get_running_loop() is not self.loop:
            raise RuntimeError("executor is closed or moved to a different event loop")
        async with self.lock:
            started = time.monotonic()
            data = program.data()
            resolution = Resolution(data, self.p)
            report = {
                "program_sha256": program.digest,
                "native_sha256": self.native_sha256,
                "status": "running",
                "trace": resolution.trace,
                "steps": [],
                "cleanup_errors": [],
                "builds": 0,
                "subprocess_attempts": [],
            }
            try:
                await self.reset(data)
                with forbid_processes() as attempts:
                    report["subprocess_attempts"] = attempts
                    async with asyncio.timeout(timeout):
                        await self.execute(data, resolution, report)
                constructed = {
                    event["id"]
                    for event in resolution.trace
                    if event["status"] == "constructed"
                }
                report["unconstructed"] = sorted(resolution.nodes.keys() - constructed)
                expected = [step["id"] for step in data["steps"]]
                if [step["id"] for step in report["steps"]] != expected:
                    raise RuntimeError(
                        "executor omitted or reordered scheduled effects"
                    )
                report["status"] = (
                    "executed"
                    if not report["unconstructed"]
                    and all(step["status"] == "observed" for step in report["steps"])
                    else "error"
                )
            except Exception as error:
                report["status"] = "incomplete"
                report["error"] = observations.error(error)
            finally:
                self.programs += 1
                report["seconds"] = time.monotonic() - started
                report["ordinal"] = self.programs
                if report["cleanup_errors"]:
                    report["status"] = "incomplete"
            return report

    async def execute(self, program, resolution, report):
        effects = None
        try:
            connection = await self.subject.acquire()
            effects = Effects(resolution, connection)
            for step in program["steps"]:
                event = {
                    "id": step["id"],
                    "operation": step["op"],
                    "status": "attempted",
                }
                report["steps"].append(event)
                try:
                    value, paths = await effects.perform(step)
                    if not paths:
                        raise RuntimeError("effect has no active native dispatch path")
                    event.update(
                        status="observed", observation=value, native_paths=paths
                    )
                except self.p.PgOrmError as error:
                    event.update(status="error", observation=observations.error(error))
                if connection.closed:
                    # Stream cancellation can discard the root connection.
                    if effects.open_transactions:
                        raise RuntimeError(
                            "connection closed with outstanding transactions"
                        )
                    connection = await self.subject.acquire()
                    effects.scopes["root"] = connection
        finally:
            if effects is not None:
                report["cleanup_errors"].extend(await effects.close())
                try:
                    await effects.scopes["root"].close()
                except Exception as error:
                    report["cleanup_errors"].append(observations.error(error))

    async def close_pools(self):
        errors = []
        for pool in (self.subject, self.reference):
            if pool is not None:
                try:
                    await pool.close()
                except Exception as error:
                    errors.append(error)
        self.subject = self.reference = None
        if errors:
            raise ExceptionGroup("campaign pool cleanup failed", errors)

    async def close(self):
        self.closed = True
        await self.close_pools()
        if (
            hashlib.sha256(self.native_file.read_bytes()).hexdigest()
            != self.native_sha256
        ):
            raise RuntimeError("extension content changed during runtime execution")

    async def __aenter__(self):
        return self

    async def __aexit__(self, *error):
        await self.close()
