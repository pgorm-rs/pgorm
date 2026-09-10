"""Detached records and explicit asynchronous stream ownership."""

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeAlias

from . import _native
from ._native import Field as Field, Record as Record

if TYPE_CHECKING:
    from .runtime import Connection

Query: TypeAlias = (
    _native.Select | _native.Insert | _native.Update | _native.Delete
    | _native.RawSQL | _native.Compiled | _native.Pipeline
    | _native.DDL | _native.CreateTable | _native.CreateIndex
)

Mapping.register(Record)


# [spec:pgorm:req:python.results]
class ResultStream:
    """An async iterator owning an active native query and its connection.

    Use ``async with await pool.stream(query) as rows`` when iteration might
    stop early. EOF releases the lease; early close or cancellation discards
    it. A stream from a Connection reserves that connection until EOF/close.
    """

    def __init__(
        self, native: _native._Stream, connection: "Connection", *, release_connection: bool,
    ) -> None:
        if not isinstance(native, _native._Stream):
            raise _native.ConstructionError("streams must be opened by a pool or connection")
        self._native = native
        self._connection = connection
        self._release_connection = release_connection
        self._closed = False

    @property
    def closed(self) -> bool:
        return self._closed or self._native.closed()

    def __aiter__(self) -> "ResultStream":
        return self

    async def __anext__(self) -> Record:
        if self._closed:
            raise StopAsyncIteration
        pending = self._native.next()
        try:
            record = await pending
        except BaseException:
            await self.aclose()
            raise
        if record is None:
            await self.aclose()
            raise StopAsyncIteration
        return record

    async def aclose(self) -> None:
        pending = self._native.close()
        self._closed = True
        try:
            await pending
        finally:
            if self._release_connection:
                await self._connection.close()

    close = aclose

    async def __aenter__(self) -> "ResultStream":
        if self.closed:
            raise _native.LifecycleError("stream is closed")
        return self

    async def __aexit__(self, *exc: Any) -> None:
        await self.aclose()

    def __repr__(self) -> str:
        return f"ResultStream(closed={self.closed})"
