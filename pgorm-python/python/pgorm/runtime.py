"""Asyncio ownership and context managers for the native PostgreSQL resources."""

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any, NamedTuple

from . import _native


class PoolStatus(NamedTuple):
    """A snapshot of native pool capacity and current usage."""

    max_size: int
    size: int
    available: int
    waiting: int


# [spec:pgorm:req:python.runtime]
# [spec:pgorm:req:python.connections]
class Pool:
    """A pool owned by the asyncio loop running at construction.

    TLS verifies the certificate and hostname by default. Explicit
    ``sslmode=disable`` in the DSN or ``tls="disable"`` selects plaintext.
    ``cafile`` supplies PEM trust anchors; otherwise the bundled WebPKI roots
    are used. A pool keeps reusable connections and one shared Tokio runtime.
    """

    def __init__(
        self,
        dsn: str,
        *,
        tls: str | None = None,
        cafile: str | Path | None = None,
        max_size: int = 10,
        connect_timeout: float = 10.0,
        acquire_timeout: float = 30.0,
        statement_cache_size: int = 128,
        recycle: str = "verified",
    ) -> None:
        if not isinstance(dsn, str):
            raise _native.ConstructionError("dsn must be a string")
        for name, value in (("max_size", max_size), ("statement_cache_size", statement_cache_size)):
            if type(value) is not int or value < 0:
                raise _native.ConstructionError(f"{name} must be a nonnegative integer")
        if isinstance(connect_timeout, bool) or isinstance(acquire_timeout, bool):
            raise _native.ConstructionError("timeouts must be numeric durations")
        try:
            ca_pem = Path(cafile).read_bytes() if cafile is not None else None
        except OSError:
            raise _native.ConstructionError("could not read the CA certificate file") from None
        try:
            self._native = _native._Pool(
                dsn, tls=tls, ca_pem=ca_pem, max_size=max_size,
                connect_timeout=connect_timeout, acquire_timeout=acquire_timeout,
                statement_cache_size=statement_cache_size, recycle=recycle,
            )
        except (TypeError, ValueError, OverflowError) as error:
            raise _native.ConstructionError(str(error)) from None

    @property
    def closed(self) -> bool:
        return self._native.closed()

    def status(self) -> PoolStatus:
        return PoolStatus(*self._native.status())

    async def acquire(self) -> "Connection":
        """Check out a connection, with a cancellable acquisition deadline."""
        return Connection(await self._native.acquire(), self)

    @asynccontextmanager
    async def connection(self) -> AsyncIterator["Connection"]:
        """Check out a connection and release it when the block exits."""
        connection = await self.acquire()
        try:
            yield connection
        finally:
            await connection.close()

    async def ping(self) -> bool:
        """Check connectivity using pgorm's ordinary cached query path."""
        async with self.connection() as connection:
            return await connection.ping()

    async def close(self) -> None:
        """Reject waiters, cancel active operations and release connections."""
        await self._native.close()

    aclose = close

    async def __aenter__(self) -> "Pool":
        try:
            await self.ping()
        except BaseException:
            await self.close()
            raise
        return self

    async def __aexit__(self, *exc: Any) -> None:
        await self.close()

    def __repr__(self) -> str:
        return f"Pool(closed={self.closed})"


class Connection:
    """One checked-out native connection; concurrent operations are errors.

    Keep the pool alive until the connection is released. Query cancellation
    discards the connection when its state cannot be established.
    """

    def __init__(self, native: _native._Connection, pool: Pool) -> None:
        if not isinstance(native, _native._Connection):
            raise _native.ConstructionError("connections must be acquired from a pool")
        self._native = native
        self._pool = pool

    @property
    def closed(self) -> bool:
        return self._native.closed()

    async def ping(self) -> bool:
        return await self._native.ping()

    async def close(self) -> None:
        await self._native.close()

    aclose = close

    async def __aenter__(self) -> "Connection":
        if self.closed:
            raise _native.LifecycleError("connection is closed")
        return self

    async def __aexit__(self, *exc: Any) -> None:
        await self.close()

    def __repr__(self) -> str:
        return f"Connection(closed={self.closed})"


async def connect(dsn: str, **options: Any) -> Pool:
    """Construct a pool and validate its PostgreSQL connection before return."""
    pool = Pool(dsn, **options)
    return await pool.__aenter__()
