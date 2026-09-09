# Executing builders and reading results

`Pool` and `Connection` execute the same native statement state returned by
`inspect()`. Accepted queries are `Select`, `Insert`, `Update`, `Delete`,
`Compiled`, and explicit `RawSQL`. Ordinary strings are not queries. No HTTP
service, query dispatcher, compiler or extension build runs per query.

```python
import os
from pgorm import Pool, Table, select, update

async def visit(account_id: int):
    accounts = Table("accounts", schema="application")
    async with Pool(os.environ["DATABASE_URL"]) as pool:
        query = update(accounts).set("visits", accounts.col("visits") + 1)
        affected = await pool.execute(query.where_(accounts.col("id") == account_id))
        account = await pool.fetch_optional(
            select(accounts.star()).from_(accounts)
            .where_(accounts.col("id") == account_id)
        )
        return affected, account
```

The application table must already exist. These two calls are separate
statements. Atomic multi-statement work requires the transaction API when
available in the installed capability manifest.

| Terminal | Result | Cardinality |
| --- | --- | --- |
| `await execute(query)` | `int` command affected count | PostgreSQL's count, including zero |
| `await fetch_all(query)` | `list[Record]` | Zero or more |
| `await fetch_one(query)` | `Record` | Exactly one, otherwise `DatabaseError` |
| `await fetch_optional(query)` | `Record \| None` | At most one, otherwise `DatabaseError` |
| `await stream(query)` | `ResultStream` async iterator | Zero or more, one pull at a time |

All three `fetch_*` terminals materialize the complete result through
`ConnectionTrait::query_all`; they do not inject a limit. Use a builder limit
when the query should be bounded, or streaming for a large result.

The executor passes the Rust-built SQL and `ValueHolder` parameters to pgorm's
ordinary cached `ConnectionTrait` methods. PostgreSQL infers placeholder types
from the statement, just as it does for Rust callers. A standalone `SELECT $1`
has insufficient context for a numeric parameter; supply a qualified cast,
for example `bind(7).cast(TypeName("int8", schema="pg_catalog"))`.

## Detached records

Records own their data after the connection closes. `row["name"]`, `get`,
`keys`, `values`, `items`, iteration and `dict(row)` expose ordinary Python
values. `row.tagged("name")` returns an immutable `Value`, retaining distinctions
such as SQL NULL versus JSON null and a nullable `int4` versus nullable `int8`.
Each access returns an independently owned Python container for JSON/arrays.

`row.fields` is an immutable tuple of `Field` objects. Fields retain output
name and position, qualified PostgreSQL type name and OID, and source table
OID/column number when PostgreSQL supplies them. Duplicate output names raise
`DecodeError`; alias projections explicitly. A left join with no matching row
returns typed NULL fields in a record. Dynamic records do not claim compiled
entity or optional-model decoding; those terminals belong to registered
entity/graph shapes advertised by the capability manifest.

The decoder uses `tokio_postgres::Row::try_get<Option<T>>` and the driver's
Rust `FromSql` implementations, the same underlying path used by pgorm's
`TryGetable`. Enum labels retain the driver's qualified `Type` identity.
INET/MAC adapters invoke the Rust `postgres_protocol` codecs also used by
pgorm's `TryGetable`. Conversions then use the public `Value` conversion path.

| PostgreSQL output | Native value tag / Python value |
| --- | --- |
| bool; internal `char`; int2/int4/int8; oid | bool; i8; i16/i32/i64; u32 |
| float4/float8 | f32/f64; Python float, including signed zero/NaN/infinity |
| text/varchar/bpchar/name; bytea | text / str; bytes |
| json/jsonb | json / dict, list, scalar or None; SQL NULL stays distinct |
| numeric | decimal / Decimal, exact 96-bit coefficient and scale 0–28 |
| uuid | uuid / UUID |
| date; time; timestamp | date; time; datetime, microsecond precision |
| timestamptz | datetime_utc / timezone-aware UTC datetime |
| inet/cidr; macaddr | ipnetwork / string; mac_address / six bytes |
| enum | enum / label, qualified `TypeName` retained |
| supported scalar array | array / list, including nullable elements |
| pgvector `vector`, when installed in PostgreSQL | vector / list of f32 values |

PostgreSQL stores a timestamptz instant without its input timezone; decoded
values therefore use UTC. Numeric values requiring rounding, non-finite
numeric, temporal values outside Python's range, `24:00:00`, and f32 NaN
payloads Python cannot preserve raise `DecodeError`. JSON uses Rust
serde_json's i64/u64/f64 number model. Numeric spellings that change value
when decoded, integers beyond i64/u64, and nesting beyond 64 levels are
rejected. JSON object ordering/whitespace are not retained.

Arrays must have at most one dimension and lower bound 1; an empty array and
an SQL NULL array remain distinct. Multi-dimensional/non-default-bound arrays,
domains, composites, ranges, intervals and unlisted types raise `DecodeError`,
including typed NULLs of unsupported types. A query/decode error never becomes
an empty list, `None`, or stream exhaustion. Server errors retain SQLSTATE;
local cardinality errors have no PostgreSQL SQLSTATE.

## Streaming and cancellation

```python
async with Pool(os.environ["DATABASE_URL"]) as pool:
    query = select(accounts.star()).from_(accounts).order_by(accounts.col("id").asc())
    async with await pool.stream(query) as rows:
        async for row in rows:
            print(row["id"])
            if row["id"] == 100:
                break
```

A pool stream checks out one connection and retains it until EOF or close.
A connection stream reserves that connection; other operations fail with
`LifecycleError` while it is active. EOF leaves the connection reusable.
Early close, decode failure or cancellation discards the incomplete connection.
Explicit `aclose()` and the context manager provide deterministic cleanup;
dropping the iterator also cancels and releases its native lease.

`ResultStream` wraps pgorm's `query_raw` / owned `RowStream`, without collecting
the result or adding a prefetch queue. The driver uses a bounded response
channel and socket backpressure; only one Python `next` operation may run at
a time. Buffer memory still depends on row/message size. This is a client
stream, not a PostgreSQL server-side cursor or a limit on server query work.

Pool or connection shutdown also interrupts an idle stream, so closing an
owner does not wait for the next Python iteration. Resources remain bound to
their original asyncio loop. `asyncio.timeout()`/task cancellation discard an
active query connection whose state is uncertain; cancellation never promises
rollback of an already submitted write. No query is retried by the binding.

## Verification

Run the installed-wheel suite through the disposable PostgreSQL wrapper in
the package README. `test_results.py` checks command counts, CRUD records,
cardinality, type metadata and exact/rejected decoding. `test_streams.py`
checks reservation, exhaustion, abandonment, cancellation and shutdown with
one-second operation budgets. Native codec parity tests run with:

```sh
PYO3_PYTHON="$PWD/target/python-dev/bin/python" \
  cargo test --manifest-path pgorm-python/Cargo.toml --target-dir target --locked --lib
```
