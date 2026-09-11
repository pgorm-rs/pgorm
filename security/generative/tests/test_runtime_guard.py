import asyncio
import subprocess
import sys
import unittest
from types import SimpleNamespace
from unittest.mock import AsyncMock

from pgorm_campaign.control_programs import read
from pgorm_campaign.executor import Executor
from pgorm_campaign.runtime_guard import forbid_processes


class RuntimeGuardTests(unittest.IsolatedAsyncioTestCase):
    # [spec:pgorm:req:generative.execution/test]
    async def test_native_panic_is_incomplete_not_a_pass(self):
        executor = object.__new__(Executor)
        executor.closed, executor.programs = False, 0
        executor.loop, executor.lock = asyncio.get_running_loop(), asyncio.Lock()
        executor.p, executor.native_sha256 = SimpleNamespace(), "unit-test"
        executor.reset = AsyncMock()
        panic = type("PanicException", (BaseException,), {"__module__": "pyo3_runtime"})
        executor.execute = AsyncMock(side_effect=panic("native panic"))
        result = await executor.run(read())
        self.assertEqual(result["status"], "incomplete")
        self.assertEqual(result["error"]["class"], "UnexpectedNativePanic")
        self.assertEqual(result["ordinal"], 1)
        executor.execute = AsyncMock(side_effect=asyncio.CancelledError())
        with self.assertRaises(asyncio.CancelledError):
            await executor.run(read())
        self.assertEqual(executor.programs, 2)

    # [spec:pgorm:req:generative.build-amortization/test]
    async def test_process_policy_follows_async_context(self):
        with forbid_processes() as attempts:
            with self.assertRaisesRegex(RuntimeError, "runtime program attempted"):
                await asyncio.create_subprocess_exec(sys.executable, "-c", "pass")
            self.assertEqual(attempts, ["subprocess.Popen"])
        result = subprocess.run([sys.executable, "-c", "pass"], check=False)
        self.assertEqual(result.returncode, 0)

    async def test_policy_does_not_capture_other_tasks(self):
        ready, proceed = asyncio.Event(), asyncio.Event()

        async def protected():
            with forbid_processes() as attempts:
                ready.set()
                await proceed.wait()
                self.assertEqual(attempts, [])

        task = asyncio.create_task(protected())
        await ready.wait()
        child = await asyncio.create_subprocess_exec(sys.executable, "-c", "pass")
        self.assertEqual(await child.wait(), 0)
        proceed.set()
        await task
