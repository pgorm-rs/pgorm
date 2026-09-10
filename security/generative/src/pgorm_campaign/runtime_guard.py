"""Runtime programs cannot launch build tools or any other subprocess."""

from contextlib import contextmanager
from contextvars import ContextVar
import sys

_active = ContextVar("pgorm_campaign_runtime", default=None)
_installed = False
PROCESS_EVENTS = frozenset(
    {
        "subprocess.Popen",
        "os.system",
        "os.posix_spawn",
        "os.exec",
        "os.fork",
        "os.forkpty",
    }
)


def _audit(event, arguments):
    attempts = _active.get()
    if attempts is not None and event in PROCESS_EVENTS:
        attempts.append(event)
        raise RuntimeError("runtime program attempted a subprocess: " + event)


# [spec:pgorm:req:generative.build-amortization]
@contextmanager
def forbid_processes():
    global _installed
    if not _installed:
        sys.addaudithook(_audit)
        _installed = True
    attempts = []
    token = _active.set(attempts)
    try:
        yield attempts
    finally:
        _active.reset(token)
