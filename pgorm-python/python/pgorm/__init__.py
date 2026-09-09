"""Compose native pgorm builders and execute them directly against PostgreSQL."""

from ._native import (
    PgOrmError,
    CancelledError,
    ConnectionError,
    ConstructionError,
    DatabaseError,
    DecodeError,
    InternalError,
    LifecycleError,
    TimeoutError,
    UnsupportedCapabilityError,
    __pgorm_version__,
    __version__,
    capabilities,
    require_capability,
)
from .runtime import Connection, Pool, PoolStatus, connect

__all__ = [
    "PgOrmError",
    "CancelledError",
    "ConnectionError",
    "ConstructionError",
    "DatabaseError",
    "DecodeError",
    "InternalError",
    "LifecycleError",
    "TimeoutError",
    "Connection",
    "Pool",
    "PoolStatus",
    "connect",
    "UnsupportedCapabilityError",
    "__pgorm_version__",
    "__version__",
    "capabilities",
    "require_capability",
]
