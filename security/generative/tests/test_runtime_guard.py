import asyncio
import subprocess
import sys
import unittest

from pgorm_campaign.runtime_guard import forbid_processes


class RuntimeGuardTests(unittest.IsolatedAsyncioTestCase):
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
