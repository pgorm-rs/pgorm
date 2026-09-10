"""Bounded subprocesses whose descendants are stopped on timeout or cancellation."""

import asyncio
from dataclasses import dataclass
import os
from pathlib import Path
import signal


class ProcessFailure(RuntimeError):
    """A command failed; arguments and environment are deliberately not echoed."""


@dataclass(frozen=True)
class Output:
    returncode: int
    stdout: str
    stderr: str


async def _read(stream, limit):
    chunks = []
    size = 0
    while chunk := await stream.read(65536):
        size += len(chunk)
        if size > limit:
            raise ProcessFailure("subprocess output exceeded its byte budget")
        chunks.append(chunk)
    return b"".join(chunks).decode("utf-8", errors="replace")


async def _write(stream, data):
    try:
        stream.write(data)
        await stream.drain()
    except (BrokenPipeError, ConnectionResetError):
        pass
    finally:
        stream.close()


async def _stop(child):
    # The leader may exit while a descendant still holds an output pipe.
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        await asyncio.wait_for(child.wait(), 1)
    except TimeoutError:
        pass
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    await child.wait()


# [spec:pgorm:req:generative.isolation]
async def run(
    *command,
    input="",
    timeout=30,
    environment=None,
    cwd=None,
    check=True,
    output_limit=2**20,
):
    if timeout <= 0 or output_limit <= 0:
        raise ValueError("subprocess budgets must be positive")
    spawn = asyncio.create_task(
        asyncio.create_subprocess_exec(
            *(str(part) for part in command),
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            start_new_session=True,
            env=environment,
            cwd=cwd,
        )
    )
    try:
        child = await asyncio.shield(spawn)
    except asyncio.CancelledError:
        child = await spawn
        await asyncio.shield(_stop(child))
        raise
    tasks = [
        asyncio.create_task(_read(child.stdout, output_limit)),
        asyncio.create_task(_read(child.stderr, output_limit)),
        asyncio.create_task(_write(child.stdin, input.encode("utf-8"))),
    ]
    try:
        async with asyncio.timeout(timeout):
            stdout, stderr, _ = await asyncio.gather(*tasks)
            code = await child.wait()
        result = Output(code, stdout, stderr)
        if check and code:
            raise ProcessFailure(f"{Path(command[0]).name} exited with status {code}")
        return result
    finally:
        await asyncio.shield(_stop(child))
        for task in tasks:
            task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)
