import asyncio
import os
from pathlib import Path
import sys
import tempfile
import unittest

from pgorm_campaign.process import ProcessFailure, run


# [spec:pgorm:req:generative.isolation/test]
class ProcessTests(unittest.IsolatedAsyncioTestCase):
    async def test_io_and_nonzero_status(self):
        result = await run(
            sys.executable,
            "-c",
            "import sys; print(sys.stdin.read()); print('diagnostic', file=sys.stderr)",
            input="雪",
        )
        self.assertEqual(result.stdout, "雪\n")
        self.assertEqual(result.stderr, "diagnostic\n")
        with self.assertRaisesRegex(ProcessFailure, "status 3"):
            await run(sys.executable, "-c", "raise SystemExit(3)")

    async def test_timeout_and_output_limit_stop_children(self):
        with self.assertRaises(TimeoutError):
            await run(sys.executable, "-c", "import time; time.sleep(30)", timeout=0.1)
        with self.assertRaisesRegex(ProcessFailure, "byte budget"):
            await run(sys.executable, "-c", "print('x' * 10000)", output_limit=100)

    async def test_cancelled_process_is_reaped(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "pid"
            script = "import os,sys,time; open(sys.argv[1], 'w').write(str(os.getpid())); time.sleep(30)"
            task = asyncio.create_task(run(sys.executable, "-c", script, path))
            async with asyncio.timeout(5):
                while not path.exists():
                    await asyncio.sleep(0.01)
            pid = int(path.read_text())
            task.cancel()
            with self.assertRaises(asyncio.CancelledError):
                await task
            with self.assertRaises(ProcessLookupError):
                os.kill(pid, 0)


if __name__ == "__main__":
    unittest.main()
