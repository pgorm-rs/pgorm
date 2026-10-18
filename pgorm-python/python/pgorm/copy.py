"""COPY's data flow: bytes into a table, and bytes out of a table or query."""
from __future__ import annotations

from collections.abc import AsyncGenerator, AsyncIterable, AsyncIterator, Buffer, Iterable
from contextlib import aclosing
from types import TracebackType
from typing import TYPE_CHECKING, Any, TypeAlias

from . import _native

if TYPE_CHECKING:
    from .runtime import Connection

CopySource: TypeAlias = Buffer | Iterable[Buffer] | AsyncIterable[Buffer]
"""What ``copy_in`` reads: bytes-like data, or a sync or async iterable of it."""

# Native writes gather small chunks and slice large ones, so each write is one
# bounded copy and a source of tiny rows costs few event-loop round trips.
_GATHER = 64 * 1024
_SLICE = 256 * 1024


def _view(item: object) -> memoryview:
    if isinstance(item, str):
        raise _native.ConstructionError("COPY data is bytes; encode text before copying it")
    if not isinstance(item, Buffer):
        raise _native.ConstructionError(
            f"COPY data must be bytes-like, not {type(item).__name__}"
        )
    try:
        return memoryview(item).cast("B")
    except TypeError:
        raise _native.ConstructionError("COPY data must be a contiguous buffer") from None


async def _single(source: Buffer) -> AsyncGenerator[memoryview]:
    yield _view(source)


async def _asynchronous(source: AsyncIterable[object]) -> AsyncGenerator[memoryview]:
    async for item in source:
        yield _view(item)


async def _synchronous(source: Iterable[object]) -> AsyncGenerator[memoryview]:
    for item in source:
        yield _view(item)


def _items(source: object) -> AsyncGenerator[memoryview]:
    """Check the source's shape before the COPY starts; items are checked as pulled."""
    if isinstance(source, str):
        raise _native.ConstructionError("COPY data is bytes; encode text before copying it")
    if isinstance(source, Buffer):
        return _single(source)
    if isinstance(source, AsyncIterable):
        return _asynchronous(source)
    if isinstance(source, Iterable):
        return _synchronous(source)
    raise _native.ConstructionError(
        "copy_in reads bytes-like data or a sync or async iterable of it, "
        f"not {type(source).__name__}"
    )


async def _pieces(items: AsyncIterator[memoryview]) -> AsyncGenerator[Buffer]:
    gathered = bytearray()
    async for view in items:
        if len(view) < _GATHER:
            gathered += view
            if len(gathered) >= _GATHER:
                yield gathered
                gathered = bytearray()
            continue
        if gathered:
            yield gathered
            gathered = bytearray()
        for start in range(0, len(view), _SLICE):
            yield view[start:start + _SLICE]
    if gathered:
        yield gathered


# [spec:pgorm:req:python.copy]
# [spec:pgorm:req:python.cancellation]
async def copy_in(
    native: _native._Connection | _native._Transaction,
    statement: _native.CopyFrom[Any],
    source: CopySource,
) -> int:
    """Run ``statement`` over ``source``, pulled only as the COPY takes data.

    Anything raised while the data is sent — by the source, by an item that is
    not bytes, or a cancellation — aborts the COPY and propagates unchanged.
    """
    items = _items(source)
    copy = await _native._copy_in(native, statement)
    try:
        async with aclosing(items), aclosing(_pieces(items)) as pieces:
            async for piece in pieces:
                if not await copy.write(piece):
                    break
    except BaseException:
        await copy.abort()
        raise
    return await copy.finish()


# [spec:pgorm:req:python.copy]
class CopyOutStream:
    """COPY TO's data as an async iterator of ``bytes`` chunks.

    A chunk is any slice of the format's byte stream: chunks already received
    are joined up to 64 KiB, and no row boundary is promised. At most a few
    chunks wait unread, so the server is held back when nothing pulls. The
    end of the data releases the connection or transaction; closing before it
    discards the connection. Use ``async with`` when reading might stop early.
    """

    def __init__(self, native: _native._CopyOut, connection: Connection | None) -> None:
        if not isinstance(native, _native._CopyOut):
            raise _native.ConstructionError(
                "copy streams must be opened by a pool, connection or transaction"
            )
        self._native = native
        self._connection = connection
        self._closed = False

    @property
    def closed(self) -> bool:
        return self._closed or self._native.closed()

    def __aiter__(self) -> CopyOutStream:
        return self

    async def __anext__(self) -> bytes:
        if self._closed:
            raise StopAsyncIteration
        pending = self._native.next()
        try:
            chunk = await pending
        except BaseException:
            await self.aclose()
            raise
        if chunk is None:
            await self.aclose()
            raise StopAsyncIteration
        return chunk

    async def aclose(self) -> None:
        pending = self._native.close()
        self._closed = True
        try:
            await pending
        finally:
            if self._connection is not None:
                await self._connection.close()

    close = aclose

    async def __aenter__(self) -> CopyOutStream:
        if self.closed:
            raise _native.LifecycleError("copy stream is closed")
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        await self.aclose()

    def __repr__(self) -> str:
        return f"CopyOutStream(closed={self.closed})"
