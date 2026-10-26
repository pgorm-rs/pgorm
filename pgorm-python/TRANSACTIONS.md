# Transactions and savepoints

Transactions run through the real Rust `DatabaseTransaction` and its native
savepoints, and, as in Rust, a transaction exists only for the code it was
opened around: in Python, one `async with` block. A scoped Rust task serves
each one from inside the closure pgorm opens it around; Python handles send
owned operations to that task, and the block's exit decides what the closure
returns, so pgorm issues the `COMMIT` or `ROLLBACK`. No borrowed lifetime is
extended and no transaction state is simulated by Python SQL strings.

```python
import asyncio
import os
import pgorm as p
from pgorm import schema as s

class RolledBack(Exception):
    pass

async def main() -> None:
    table = p.Table("python_transaction_example")
    async with p.Pool(os.environ["PGORM_TEST_DSN"]) as pool:
        await pool.execute(s.create_table(table).column(s.ColumnDef("id", "integer")).primary_key("id"))
        try:
            async with pool.transaction() as tx:
                await tx.execute(p.insert(table).columns("id").values(1))
                try:
                    async with tx.transaction() as child:
                        await child.execute(p.insert(table).columns("id").values(2))
                        raise RolledBack
                except RolledBack:
                    pass
                assert [r["id"] for r in await tx.fetch_all(p.select(p.col("id")).from_(table))] == [1]
            assert (await pool.fetch_one(p.select(p.col("id")).from_(table)))["id"] == 1
        finally:
            await pool.execute(s.drop_table(table))

if __name__ == "__main__":
    asyncio.run(main())
```

`async with connection.transaction()` opens a transaction on a connection, and
`async with pool.transaction()` on a connection of its own. A normal block exit
commits; an exception rolls back and propagates, so raising is how a block
rolls back. The block is the transaction's only scope: a `Transaction` is not
itself a context manager, and only its block's exit ends it. Using it after
the block raises `LifecycleError`.

A commit that fails discards the connection and raises its error from the
block's exit. So does a `ROLLBACK` that fails: its error is raised in place of
the block's exception, which it carries as `__context__`. A cancellation,
`KeyboardInterrupt` or `SystemExit` is never replaced, so that asyncio still
sees the cancellation as itself; the connection is discarded all the same.
A block whose transaction was already discarded, by a cancelled request or a
COPY the server refused, keeps its own exception, and a normal exit from it
raises `LifecycleError`, since nothing can commit.

`async with tx.transaction()` opens a nested savepoint: a normal exit releases
it, and an exception rolls back to it and propagates into the enclosing block,
which can catch it and continue. While a child is open, its parent cannot
query or open another child. While a transaction is open, its acquired
connection cannot perform other operations. Overlapping requests fail with
`LifecycleError`; the bindings do not queue concurrent transaction use. A block
exits only after releasing its parent.

Use the same `execute`, `fetch_all`, `fetch_one` and `fetch_optional` methods as
on a connection. These accept SELECT/CRUD builders, pipelines, schema statements,
`Compiled` and explicit `RawSQL`. Dynamic record cardinality rules are unchanged.
Runtime models, registered entity queries and ActiveModel hooks, registered
graphs and cursors, and pipeline source tuples also accept `Transaction`.
Transaction streaming is not exposed; the installed `transaction_policy`
reports this explicitly. Pool and connection streams retain their existing API.

## Native transaction modes

`connection.transaction()` and `pool.transaction()` accept these native choices:

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
the outer transaction: the error leaving the savepoint's block rolls it back,
and the enclosing block catches it and continues.

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

A block's context manager entered by hand and dropped without exiting rolls
its transaction back when asyncio finalizes its generator. A transaction
handle dropped while still open
requests native rollback; that abandoned idle rollback has a five-second
budget, after which the connection is discarded.
The integration tests use two-second budgets for cancellation and shutdown on
the dedicated PostgreSQL fixture; the ownership rules do not depend on Python
garbage collection timing.
