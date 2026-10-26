import asyncio
import errno
import os
import signal
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from pgorm_campaign import process
from pgorm_campaign.process import ProcessFailure, run


async def _zombie_leader():
    # The event loop reaps its children, so blocking it until the leader has
    # died leaves the leader a zombie, the only member of its group, when the
    # next statement runs.
    child = await asyncio.create_subprocess_exec(
        sys.executable, "-c", "import time; time.sleep(30)", start_new_session=True
    )
    os.kill(child.pid, signal.SIGKILL)
    os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOWAIT)  # noqa: ASYNC222
    return child


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

    async def test_stop_reaps_an_exited_unreaped_leader(self):
        child = await _zombie_leader()
        await process._stop(child)
        self.assertEqual(child.returncode, -signal.SIGKILL)
        with self.assertRaises(ProcessLookupError):
            os.killpg(child.pid, 0)

    async def test_stop_raises_eperm_from_a_live_group(self):
        refused = PermissionError(errno.EPERM, os.strerror(errno.EPERM))
        refusing = mock.patch("os.killpg", side_effect=refused)
        child = await asyncio.create_subprocess_exec(
            sys.executable, "-c", "import time; time.sleep(30)", start_new_session=True
        )
        try:
            with (
                mock.patch.object(process, "_REFUSAL_GRACE", 0.1),
                refusing,
                self.assertRaises(PermissionError),
            ):
                await process._stop(child)
            self.assertIsNone(child.returncode)
        finally:
            child.kill()
            await child.wait()
        # A group that still refuses once its leader is reaped holds a member
        # this process cannot stop.
        child = await _zombie_leader()
        with (
            mock.patch.object(process, "_REFUSAL_GRACE", 0.1),
            refusing,
            self.assertRaises(PermissionError),
        ):
            await process._stop(child)
        self.assertEqual(child.returncode, -signal.SIGKILL)


if __name__ == "__main__":
    unittest.main()
