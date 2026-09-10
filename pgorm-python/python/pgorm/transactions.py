"""Explicit transaction and savepoint ownership over native Rust scopes."""
from __future__ import annotations

from contextlib import asynccontextmanager
from collections.abc import AsyncIterator
from types import TracebackType
from typing import TYPE_CHECKING, Literal, TypeAlias

from . import _native
from .results import Query, Record

if TYPE_CHECKING:
    from .runtime import Connection

TransactionMode: TypeAlias = Literal["default", "read_write", "read_only", "deferrable"]
IsolationLevel: TypeAlias = Literal["read_uncommitted", "read_committed", "repeatable_read", "serializable"]


# [spec:pgorm:req:python.transactions]
class Transaction:
    """One native transaction; its parent is reserved until commit or rollback.

    ``async with await connection.begin() as tx`` commits on normal exit and
    rolls back on exceptions. A nested ``tx.transaction()`` is a real savepoint.
    """

    def __init__(self, native: _native._Transaction, parent: Connection | Transaction) -> None:
        if not isinstance(native, _native._Transaction) or not native.owner_matches(getattr(parent, "_native", None)):
            raise _native.ConstructionError("transactions must be opened on their actual parent")
        self._native = native
        self._parent = parent

    @property
    def closed(self) -> bool:
        return self._native.closed()

    async def begin(self) -> Transaction:
        """Open a savepoint and reserve this parent until the child finishes."""
        return Transaction(await self._native.begin(), self)

    @asynccontextmanager
    async def transaction(self) -> AsyncIterator[Transaction]:
        async with await self.begin() as transaction:
            yield transaction

    async def execute(self, query: Query) -> int:
        return await self._native.execute(query)

    async def fetch_all(self, query: Query) -> list[Record]:
        return await self._native.fetch(query, "all")

    async def fetch_one(self, query: Query) -> Record:
        return await self._native.fetch(query, "one")

    async def fetch_optional(self, query: Query) -> Record | None:
        return await self._native.fetch(query, "optional")

    async def commit(self) -> None:
        await self._native.finish(True)

    async def rollback(self) -> None:
        await self._native.finish(False)

    async def close(self) -> None:
        """Roll back an open transaction and wait for its scope to be released."""
        if not self.closed:
            try:
                await self.rollback()
            except BaseException:
                await self._native.abort()
                raise
        await self._native.wait_closed()

    aclose = close

    async def __aenter__(self) -> Transaction:
        if self.closed:
            raise _native.LifecycleError("transaction is closed")
        return self

    async def __aexit__(self, exc_type: type[BaseException] | None, exc: BaseException | None, traceback: TracebackType | None) -> None:
        if exc_type is None:
            try:
                await self.commit()
            except BaseException:
                await self._native.abort()
                raise
        else:
            try:
                await self.close()
            except _native.PgOrmError:
                # Cleanup must not replace the block's original exception.
                # A failed/cancelled native finish discards the connection.
                pass

    def __repr__(self) -> str:
        return f"Transaction(closed={self.closed})"
