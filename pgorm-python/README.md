# pgorm for Python

The `pgorm` Python package loads an optional PyO3 extension over pgorm's Rust
builders. Calls run in the Python process. PostgreSQL uses its native protocol;
there is no HTTP adapter or query dispatcher.

The package is under implementation. `pgorm.capabilities()` reports only the
operations present in the installed build. The full contract is in
`docs/spec/python.md` in the repository; unimplemented operations are not
claimed by the capability manifest.

## Build and install

From the repository root, using CPython 3.14:

```sh
uv venv target/python-dev
uv pip install --python target/python-dev/bin/python 'maturin==1.15.0'
target/python-dev/bin/maturin build --manifest-path pgorm-python/Cargo.toml \
  --interpreter target/python-dev/bin/python --out target/python-dist
uv pip install --python target/python-dev/bin/python target/python-dist/*.whl
target/python-dev/bin/python pgorm-python/tests/with_postgres.py \
  target/python-dev/bin/python -m unittest discover -s pgorm-python/tests
```

`maturin sdist --manifest-path pgorm-python/Cargo.toml --out target/python-dist`
builds the source distribution. Build and install commands do not publish to a
registry. `support.json` records the ABI and candidate platform matrix; release
artifacts require the installation checks on each claimed platform. Registry
name availability must be checked before publication.

Python support has its own Cargo workspace, lockfile and build command. A normal
`cargo build --workspace` at the repository root does not load PyO3 or Python
configuration. CPython uses version-specific wheels; free-threaded builds and
subinterpreters are outside the supported matrix.

```python
import pgorm

print(pgorm.__version__)
print(pgorm.capabilities())
```

## Values

`Value(data, kind)` constructs an immutable native Rust value. Explicit kinds
retain integer widths, typed NULLs, temporal variants and qualified enum names;
`Value.array` and `Value.json` preserve array identity and JSON null. See the
[conversion table and policies](VALUES.md) for exact limits and examples.

```python
from pgorm import Value

value = Value(42, "i16")
assert value.value == 42
assert value.snapshot()["type"] == {"kind": "i16"}
assert Value.json(None) != Value.null("json")
```

## Expressions

Compose native expressions with `col`, `bind`, `literal`, `Condition` and
`call`. Operations return reusable, immutable Rust builder state. Inspection
returns Rust-built SQL and tagged parameters:

```python
from pgorm import col, literal

predicate = (col("active") == True) & (col("score") > literal(10))
compiled = predicate.inspect()
print(compiled.sql)
print([value.snapshot() for value in compiled.params])
```

See [expression construction and Rust API mappings](EXPRESSIONS.md) for
conditions, functions, literal/bound paths, casts and ownership semantics.

## Statements

`Table`, `select`, `insert`, `update` and `delete` compose the Rust statement
builders over application names supplied at runtime. `inspect()` returns the
SQL and parameters produced by that builder state. See [runtime statement
examples](STATEMENTS.md) for joins, grouping, write guards, conflict actions,
RETURNING and the explicit `RawSQL` template API.

`await pool.execute(query)` returns affected rows. `fetch_all`, `fetch_one`
and `fetch_optional` return detached records; `stream` opens an async iterator.
See [execution, result types and stream ownership](RESULTS.md) for examples,
PostgreSQL decoding limits and cancellation behavior.

The [direct builder integration suite](DIRECT_BUILDERS.md) installs a wheel in
a fresh Python environment, runs 28 application query programs against
PostgreSQL, and compares their SQL and tagged parameters with independent Rust
builders. It provides the focused proof of direct Python access without HTTP.

Applications with Rust entities can also [compile their entity registrations
into an application wheel](ENTITIES.md). Python then uses their real typed
queries, models, ActiveValue states and write hooks through `pgorm.entity`.

## Connections

Construct and use resources inside one running asyncio loop. Database waiting
runs on the shared Tokio runtime. Connection checkout supports cancellation and
an acquisition timeout; one connection rejects concurrent operations.

```python
import asyncio
import os
import pgorm

async def main():
    async with pgorm.Pool(os.environ["DATABASE_URL"]) as pool:
        async with pool.connection() as connection:
            assert await connection.ping()

asyncio.run(main())
```

TLS verifies certificates and hostnames using WebPKI roots, or the PEM CA file
passed as `cafile`. Set `tls="disable"` or include `sslmode=disable` in the DSN
for an explicitly plaintext connection. TLS verification never falls back to
plaintext. Pool sizing, connect/acquisition deadlines, statement-cache bounds
and verified/fast recycling are keyword options on `Pool`.

Closing a pool rejects waiters and cancels operations on its checked-out
connections. Closing a connection releases it. If an operation is cancelled
while its database state is uncertain, its connection is discarded. Cancellation
does not promise rollback of an already submitted write. Context managers close
their resources on normal and exceptional exit; explicit `close()`/`aclose()`
is also available. Resources cannot migrate to another event loop.

Construction, capability, connection, database, decode, timeout and lifecycle
failures have distinct exception classes under `PgOrmError`. PostgreSQL server
errors retain `sqlstate`, message and structured diagnostics with connection
credentials redacted. Asyncio cancellation remains `asyncio.CancelledError`.
Internal Rust panics retain PyO3's identifiable panic exception and are not
classified as invalid application input.

The test wrapper provisions its own PostgreSQL server and CA certificate, runs
the supplied command with `PGORM_TEST_DSN` and `PGORM_TEST_CA`, and removes the
server afterward. It requires Docker and OpenSSL only for testing. Existing
fixtures can instead supply those two environment variables directly.
