# Types and a runnable application

The installed package includes `py.typed`, native `.pyi` declarations and typed
Python resource/model wrappers. Use CPython 3.14. The public API is checked with
mypy 1.18.2 in strict mode against an installed wheel.

[The ordinary application](examples/application.py) uses only `import pgorm`
and its `schema` and `pipeline` modules. It creates two uniquely named tables,
inserts and updates records, joins them, commits a transaction while rolling
back a nested savepoint, selects through a pipeline, streams results and
deletes a record. Its `finally` blocks remove its own tables. It needs a
PostgreSQL database on which the supplied account can create and drop tables.

With the wheel installed in your current Python environment, run it from the
repository root:

```sh
export PGORM_TEST_DSN="$DATABASE_URL"
python -I pgorm-python/examples/application.py
uv tool run --from mypy==1.18.2 mypy --strict \
  --python-executable "$(command -v python)" pgorm-python/examples/application.py
```

The output includes the two joined rows, streamed IDs `[1, 2]`, one deleted
record and transport `in-process`. The example needs no HTTP service or
security harness. See [schema](SCHEMA.md), [statements](STATEMENTS.md),
[transactions](TRANSACTIONS.md), [pipelines](PIPELINE.md) and
[stream ownership](RESULTS.md) for the corresponding API details.

## Return types and ownership

| Operation | Result after `await` |
| --- | --- |
| `pool.execute(query)`, `connection.execute(query)`, `transaction.execute(query)` | `int` affected rows |
| `fetch_all(query)` | `list[Record]` |
| `fetch_one(query)` | `Record`; raises `DatabaseError` unless exactly one row exists |
| `fetch_optional(query)` | `Record \| None`; raises `DatabaseError` if more than one row exists |
| `pool.stream(query)`, `connection.stream(query)` | `ResultStream`, an `AsyncIterator[Record]` and async context manager |
| `connection.begin()`, `transaction.begin()` | `Transaction` |
| `Model.find().all(executor)` | `list[ModelRecord]` |
| `Model.find().one_opt(executor)` | `ModelRecord \| None` |
| `Entity.find().all(connection_or_transaction)` | `list[EntityModel]` |
| `Pipeline.all(connection_or_transaction)` | `list[Record]` |
| `Pipeline.one_opt(connection_or_transaction)` | `Record \| None` |

`Pool.connection()` and `Pool.transaction()` return async context managers;
use `async with` directly. Queries and builder methods are synchronous and
immutable until an execution method is called. Native methods returning an
`Awaitable[T]` must be awaited just like the Python `async def` facade methods.
The stream must be closed or used with `async with` when iteration can stop
early. A transaction reserves its connection; a savepoint reserves its parent
transaction until it finishes. Transaction streaming is not exposed.

The stubs mark positional-only parameters and keyword-only options. An opaque
native result such as `Record`, `Compiled` or `Expr` is obtained from its
builder/operation. Its stub constructor takes `Never` to reject direct
construction, matching the native runtime's `TypeError`.

## Dynamic columns and concrete application models

`Record` and runtime `ModelRecord` are mappings with dynamically named fields.
Their values are `Any`: a type checker cannot infer a SQL projection or a
runtime schema declaration. Optional record results still require checking for
`None`. Explicit `Value` instances preserve runtime integer widths, typed
NULLs, array identities and qualified enums; Python's `int` annotation does not
encode SQL widths or bounds. These remain runtime validation rules.

For concrete model properties and typed graph tuples, follow the optional
[compiled entity and code generation workflow](CODEGEN.md). Its final wheel
exports an application module such as `pgorm.app`, with properties including
`int`, `str`, `str | None` and graph slots such as `NoteModel | None`. Generated
write setters accept their reflected type or an explicit `Value`. Unknown
custom Rust representations and JSON retain `Any`. The generated module is
checked against its installed extension's version and registry metadata.

## Errors

Python typing does not declare checked exceptions. Invalid argument shapes may
raise `TypeError`; invalid supported values or builder states raise
`ConstructionError`. Unsupported installed capabilities raise
`UnsupportedCapabilityError`. Network/TLS setup uses `ConnectionError`, server
errors and cardinality failures use `DatabaseError`, and failed decoding uses
`DecodeError`. `DatabaseError` exposes optional `sqlstate`, `message`, `severity`
and diagnostic fields such as `constraint` and `detail`. Server errors populate
these fields; client-side cardinality failures leave them as `None`. Use
`str(error)` for the exception text in either case.

Closed resources, concurrent use of a reserved connection and use from another
event loop raise `LifecycleError`; configured deadlines raise pgorm's
`TimeoutError`. These errors derive from `PgOrmError`. Task cancellation remains
`asyncio.CancelledError` and should normally propagate. Rust panic exceptions
are not converted into input-validation errors. See [connections](README.md#connections)
and [transaction cleanup](TRANSACTIONS.md) for resource behavior on failure.

## Verify the installed package

With an existing database and a wheel installed in `target/python-check`:

```sh
target/python-dev/bin/python pgorm-python/checks/typing.py \
  --python target/python-check/bin/python
```

Alternatively, wrap that command with `tests/with_postgres.py` to provision a
disposable test database. The checker compares installed native signatures
with shipped stubs, verifies `py.typed`, runs strict mypy on the application,
checks each expected diagnostic in the invalid consumer, and executes the
application. It removes `PYTHONPATH`/`MYPYPATH`, uses isolated Python launches,
and records the installed native path and stub hashes in
`target/python-typing/summary.json`. The separate
[code generation checker](CODEGEN.md#verify-the-workflow) builds and checks the
concrete application types, including invalid assignments and optional slots.
