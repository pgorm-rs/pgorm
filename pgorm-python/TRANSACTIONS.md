# Transactions and savepoints

Transactions run through the real Rust `DatabaseTransaction` and its native
savepoints. A scoped Rust task owns the borrowed connection; Python handles
send owned operations to that task. No borrowed lifetime is extended and no
transaction state is simulated by Python SQL strings.

```python
import asyncio
import os
import pgorm as p
from pgorm import schema as s

async def main() -> None:
    table = p.Table("python_transaction_example")
    async with p.Pool(os.environ["PGORM_TEST_DSN"]) as pool:
        await pool.execute(s.create_table(table).column(s.ColumnDef("id", "integer").primary_key()))
        try:
            async with pool.transaction() as tx:
                await tx.execute(p.insert(table).columns("id").values(1))
                child = await tx.begin()
                await child.execute(p.insert(table).columns("id").values(2))
                await child.rollback()
                assert [r["id"] for r in await tx.fetch_all(p.select(p.col("id")).from_(table))] == [1]
            assert (await pool.fetch_one(p.select(p.col("id")).from_(table)))["id"] == 1
        finally:
            await pool.execute(s.drop_table(table))

if __name__ == "__main__":
    asyncio.run(main())
```

`await connection.begin()` opens an explicit transaction. Call `await tx.commit()`
or `await tx.rollback()` once. `async with connection.transaction()` and
`async with pool.transaction()` open and finish the scope automatically.
`async with await connection.begin()` also works. A normal block exit commits;
an exceptional exit rolls back. Cleanup preserves the block's original exception.
Calling commit or rollback again on a closed transaction raises `LifecycleError`.
`await tx.close()` is idempotent, rolls back an open scope and waits for release.

`await tx.begin()` creates a nested savepoint. `async with tx.transaction()` is
the corresponding context manager. While a child is open, its parent cannot
query, commit, roll back or open another child. While a transaction is open,
its acquired connection cannot perform other operations. Overlapping requests
fail with `LifecycleError`; the bindings do not queue concurrent transaction
use. A successful commit/rollback returns only after releasing the parent.

Use the same `execute`, `fetch_all`, `fetch_one` and `fetch_optional` methods as
on a connection. These accept SELECT/CRUD builders, pipelines, schema statements,
`Compiled` and explicit `RawSQL`. Dynamic record cardinality rules are unchanged.
Runtime models, registered entity queries and ActiveModel hooks, registered
graphs and cursors, and pipeline source tuples also accept `Transaction`.
Transaction streaming is not exposed; the installed `transaction_policy`
reports this explicitly. Pool and connection streams retain their existing API.

## Native transaction modes

The outer `begin` and `transaction` methods accept these native choices:

| Mode | Meaning | `isolation=` |
| --- | --- | --- |
| `default` | Inherit the PostgreSQL session defaults | Omit |
| `read_write` | Explicit READ WRITE | Optional |
| `read_only` | Explicit READ ONLY | Optional |
| `deferrable` | SERIALIZABLE READ ONLY DEFERRABLE | Omit |

Isolation names are `read_uncommitted`, `read_committed`, `repeatable_read` and
`serializable`. PostgreSQL determines their behavior. Nested savepoints inherit
the transaction configuration and take no mode or isolation arguments.

```python
async def read_snapshot(connection: p.Connection) -> None:
    async with connection.transaction(mode="read_only", isolation="repeatable_read") as tx:
        row = await tx.fetch_one(p.select(p.literal("snapshot").as_("label")))
        print(row["label"])
```

Database errors preserve SQLSTATE and the existing diagnostics. Native
transaction statements evict a rejected cached plan without retrying a failed
transaction. There is no additional write retry or automatic retry of the
transaction body. Use a savepoint when handling a database error and continuing
the outer transaction; roll back the failed savepoint before continuing.

## Cancellation and cleanup

Cancelling an in-flight request invalidates the entire connection, including
its enclosing transaction and savepoints. The owner task drops the native
borrowed scopes and discards the connection. A cancelled write can have an
unknown outcome; cancellation does not assert that it was rolled back.

Cancelling a block while no native operation is active allows its context
manager to await rollback normally. Leaving a block with a concurrent query or
an unfinished child discards the connection when normal completion cannot
recover it. Pool or connection shutdown interrupts both active and idle
transaction owners and waits for the leased connection to be released.

Dropping an idle Python transaction without explicit cleanup requests native
rollback. Abandoned idle rollback has a five-second budget, after which the
connection is discarded. Prefer context managers or explicit awaited cleanup.
The integration tests use two-second budgets for cancellation and shutdown on
the dedicated PostgreSQL fixture; the ownership rules do not depend on Python
garbage collection timing.
