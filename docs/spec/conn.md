# Connections, Pooling, and Transactions

pgorm's connection layer is a thin composition over a vendored deadpool-postgres
fork (`pgorm-pool`) and `tokio-postgres`. `src/database/` exposes the pool and
connection handles plus the `ConnectionTrait` / `TransactionTrait` surface;
`pgorm-pool/src/` owns connection lifecycle, recycling, and statement caching.

## Pool construction and access

> [spec:pgorm:req:conn.pool+1]
> `connect(config: tokio_postgres::Config) -> DatabasePool` MUST construct a
> deadpool-backed pool wrapping `pgorm_pool::Pool`: a `Manager` built from the
> given config with `NoTls`, `RecyclingMethod::Fast`, and no tag, combined with
> the default deadpool pool configuration. Its signature is infallible by
> design, not by omission: `config` shapes the `Manager` and no caller input
> reaches the pool builder, so the only way the build can fail is if pgorm's own
> defaults are invalid. That is an internal invariant and MUST panic.
>
> `connect_with_builder(config, build)` MUST apply the caller's closure to the
> `PoolBuilder` before building, allowing pool sizing and timeout customization,
> and MUST return `Result<DatabasePool, DbErr>`. Because the closure is caller
> input, a builder shaped into an unbuildable pool — deadpool rejects timeouts
> configured without a runtime — MUST surface as `DbErr::Custom` carrying the
> builder's message, never as a panic.
>
> TLS connections are not supported through these entry points (`NoTls` is
> hard-coded); a custom-TLS pool requires assembling a `pgorm_pool::Manager`
> directly.

> [spec:pgorm:sem:conn.pool.get]
> `DatabasePool::get()` asynchronously acquires a pooled connection and wraps
> it as `DatabaseConnection`; acquisition failure surfaces as
> `DbErr::Pool(pgorm_pool::PoolError)` via `From`. `DatabasePool::status()`
> returns the live `deadpool::Status` (size, available, waiting) and
> `DatabasePool::tag()` returns the pool's tag as `Arc<String>`. An untagged
> pool receives a generated default tag of the form `default-{n}` from a
> process-wide monotonic counter in `pgorm-pool`.

> [spec:pgorm:req:conn.pool.no-conn-trait]
> `DatabasePool` MUST NOT implement `ConnectionTrait` (nor `Deref` to the inner
> pool). Executing a statement always requires explicitly acquiring a
> `DatabaseConnection` via `DatabasePool::get()` first. This is deliberate:
> implicitly checking out a fresh connection per statement hides pool churn and
> breaks transactional reasoning, so the pool-as-connection pattern inherited
> from SeaORM was removed.

> [spec:pgorm:sem:conn.pool.multi+1]
> `connect_multi_with_builder(config, builders)` builds one pool per entry of a
> `BTreeMap<String, builder-fn>`: each pool clones the shared
> `tokio_postgres::Config`, is tagged with its map key, and uses
> `RecyclingMethod::Fast`; the result is a
> `Result<BTreeMap<Arc<String>, DatabasePool>, DbErr>` keyed by tag. Every
> builder is caller input, so construction is fallible on the same terms as
> `connect_with_builder` (`conn.pool`): the first entry whose builder cannot
> produce a pool aborts the whole construction with `DbErr::Custom` and no map
> is returned. `DatabaseMultiPool` wraps the same map shape and offers
> `get(key) -> Option<DatabasePool>` (a cheap clone of the refcounted pool) and
> `status()` returning per-tag `deadpool::Status`.
>
> Selection is explicit by tag key only — there is no round-robin, load
> balancing, or fallback across pools. `DatabaseMultiPool` currently has no
> public constructor (its field is crate-private and
> `connect_multi_with_builder` returns the raw `BTreeMap` on success), so
> outside the crate it is effectively read-only API surface.

## Connection lifecycle in pgorm-pool

> [spec:pgorm:sem:conn.pool.lifecycle]
> `Manager::create` connects via `tokio_postgres`, spawns the connection
> driver future onto a tokio task, and hands out a `ClientWrapper` owning the
> client, the task's `JoinHandle`, and a fresh per-connection
> `StatementCache`; the cache is registered with the manager's
> `StatementCaches` registry. Dropping a `ClientWrapper` aborts its connection
> task. `Manager::recycle` rejects clients whose `is_closed()` is true and
> otherwise runs the recycling method's query, if any; `detach` unregisters
> the connection's statement cache.

> [spec:pgorm:sem:conn.pool.recycle]
> `RecyclingMethod` selects the health check run on reuse: `Fast` (the
> default, and what `connect` uses) performs no query — only the `is_closed()`
> check; `Verified` executes an empty `simple_query` round trip; `Clean` runs
> a `DISCARD ALL`-like statement sequence (`CLOSE ALL; SET SESSION
> AUTHORIZATION DEFAULT; RESET ALL; UNLISTEN *; SELECT
> pg_advisory_unlock_all(); DISCARD TEMP; DISCARD SEQUENCES;`) that
> intentionally omits `DEALLOCATE ALL`/`DISCARD PLANS` so cached prepared
> statements survive recycling; `Custom(sql)` runs caller-provided SQL.

> [spec:pgorm:sem:conn.pool.statement-cache]
> Each `ClientWrapper` carries an `Arc<StatementCache>` keyed by `(query text,
> parameter types)`. `prepare_cached` / `prepare_typed_cached` return the
> cached `tokio_postgres::Statement` on hit and prepare-then-insert on miss.
> `Transaction`s, nested transactions, and savepoints created through the
> wrapper share the owning client's cache. The manager-level `StatementCaches`
> holds weak references to every live cache and supports `clear()` and
> `remove(query, types)` across all pooled connections.
>
> The cache is opt-in: pgorm's `ConnectionTrait` methods pass statements
> straight to `tokio_postgres` (which prepares internally per call site) and do
> not consult the `StatementCache`; only callers invoking `prepare_cached` /
> `prepare_typed_cached` on the wrapper types benefit from it.

## Statement execution surface

> [spec:pgorm:def:conn.pool.conn-trait+2]
> `ConnectionTrait` is the uniform statement-execution surface over
> connections and transactions. It defines seven async methods. Six are
> generic over `T: ?Sized + ToStatement + Send + Sync` with parameter binding
> (no string interpolation): `execute(stmt, params) -> u64` (affected-row
> count), `execute_raw(stmt, params)` taking an `ExactSizeIterator` of
> `BorrowToSql` values instead of a `&[&dyn ToSql]` slice, `query_one ->
> Row` (errors if not exactly one row), `query_opt -> Option<Row>`,
> `query_all -> Vec<Row>`, and `query_raw(stmt, params) -> RowStream`,
> which takes the same `BorrowToSql` iterator as `execute_raw` and returns
> the unbuffered row stream of `exec.stream`. Errors map to
> `DbErr::Postgres`.
>
> The seventh, `batch_execute(sql: &str) -> ()`, is neither generic nor
> parameterized: it sends `sql` through the simple-query protocol, so the
> string MAY hold several `;`-separated statements, and it returns no rows.
> It is the only method that accepts a multi-statement string — the other six
> go through the extended protocol, where a prepared statement carries exactly
> one command and PostgreSQL rejects a second with *cannot insert multiple
> commands into a prepared statement*. Execution stops at the first statement
> that fails. Nothing is prepared, so the statement cache
> (`conn.pool.statement-cache`) is bypassed by construction rather than by
> policy. It exists for DDL, migration, and fixture surfaces, where a script
> is the unit of work; since values can reach it only by interpolation, `sql`
> must be built from trusted input.
>
> It is implemented for `DatabaseConnection`, `&DatabaseConnection`, and
> `DatabaseTransaction`, each delegating directly to the underlying
> `pgorm-pool` client or transaction — `batch_execute` through
> `GenericClient::batch_execute` (`conn.pool.generic-client`), which is
> otherwise unreachable from pgorm because the wrapper types' inner fields are
> crate-private.

> [spec:pgorm:def:conn.pool.generic-client]
> `pgorm_pool::GenericClient` is the sealed trait (`Sync` + a private
> `Sealed` supertrait) unifying `Client` — the deadpool `Object` wrapping a
> `ClientWrapper` — and `Transaction<'_>` behind one statement surface. It
> is a 1:1 copy of `tokio_postgres::GenericClient` as of tokio-postgres
> 0.7.7 with two deliberate changes: the `client()` accessor is removed,
> and `prepare_cached` / `prepare_typed_cached` are added. Sealing limits
> implementors to exactly those two types.
>
> Its methods — `execute`, `execute_raw`, `query`, `query_one`,
> `query_opt`, `query_raw`, `prepare`, `prepare_typed`, `prepare_cached`,
> `prepare_typed_cached`, `transaction(&mut self)`, `batch_execute` —
> delegate directly to the corresponding `tokio_postgres::Client` /
> `tokio_postgres::Transaction` method, except the cached prepares and
> `transaction`, which route through the pool wrapper types
> (`ClientWrapper`, pgorm-pool's `Transaction`) so both levels share the
> per-connection `StatementCache` (`conn.pool.statement-cache`) and nested
> transactions go through the wrapper's savepoint logic.

## Transactions

> [spec:pgorm:req:conn.tx+1]
> `TransactionTrait::begin(&mut self)` MUST return a `DatabaseTransaction<'_>`
> borrowing the parent exclusively, so no other statement can run on the
> connection while the transaction handle is alive. `DatabaseConnection::begin`
> issues `BEGIN` on the pooled client; `DatabaseTransaction::begin` creates a
> nested transaction, which `tokio_postgres` implements as a savepoint. Both
> transaction levels share the connection's statement cache.
>
> `DatabaseConnection::begin_with(opts: TransactionOptions)` is the configured
> counterpart, issuing `BEGIN` through `pgorm_pool`'s `TransactionBuilder`.
> `TransactionOptions` is a `Debug + Clone + Copy + Default` struct of exactly
> three public fields — `isolation_level: Option<IsolationLevel>`, `read_only:
> bool`, `deferrable: bool` — and each is forwarded to the builder only when
> set, so `TransactionOptions::default()` produces the same plain `BEGIN` as
> `begin` and unset options inherit the session defaults rather than being
> pinned to `READ WRITE` / `NOT DEFERRABLE`. `IsolationLevel` is
> `tokio_postgres::IsolationLevel`, re-exported from `pgorm`.
>
> `begin_with` is an inherent method on `DatabaseConnection`, deliberately NOT
> on `TransactionTrait`: the trait's other implementor is
> `DatabaseTransaction`, whose `begin` is a savepoint, and `SAVEPOINT` accepts
> no isolation level, access mode, or deferrability. Putting the configured
> form on the trait would require the savepoint implementation to ignore or
> reject its argument, so it is offered only where PostgreSQL honours it.

> [spec:pgorm:sem:conn.tx.guard+1]
> `DatabaseTransaction` wraps `Option<pgorm_pool::Transaction>` as a
> commit-or-rollback guard. `commit(self)` and `rollback(self)` each consume
> the handle, `take()` the inner transaction, and await `COMMIT` / `ROLLBACK`
> respectively, mapping failure to `DbErr::Postgres`. Because both take the
> `Option`, the `Drop` impl's
> `tracing::warn!("Transaction dropped without committing!")` fires only on
> the implicit path — neither `commit` nor `rollback` was called.
>
> The two rollback paths are not equivalent. Explicit `rollback` awaits the
> round trip, so a failure reaches the caller. The implicit path — dropping an
> uncommitted handle — is fire-and-forget: `tokio_postgres::Transaction::drop`
> synchronously enqueues a raw `ROLLBACK` (`ROLLBACK TO <savepoint>` when
> nested) onto the connection's unbounded request channel and discards both the
> response stream and any send error, so the rollback's outcome is
> unobservable and the warning is the only trace of it. The
> uncommitted-transaction check is a runtime warning, not a compile-time or
> panic-level guarantee.
>
> The queued rollback is nevertheless correctly ordered against subsequent work
> on the same connection. The enqueue happens inside `Drop`, and the exclusive
> `&mut` borrow the transaction holds on the client (`conn.tx`) is not released
> until that drop completes, so no later statement can reach the channel first;
> the connection driver task drains the channel in FIFO order. That task is
> owned by `ClientWrapper` and aborted only when the wrapper is dropped
> (`conn.pool.lifecycle`) — pool recycling borrows the wrapper rather than
> replacing it — so a `ROLLBACK` queued as the handle drops is still flushed
> after the connection returns to the pool and is handed to the next caller.

> [spec:pgorm:sem:conn.tx.closure]
> `DatabaseConnection::transaction(f)` and `transaction_with(opts, f)` run a
> closure inside a transaction, taking `F: AsyncFnOnce(&mut
> DatabaseTransaction<'s>) -> Result<T, E>` — a native `AsyncFn*` bound, not a
> boxed future — where `'s` is the lifetime of the `&'s mut self` receiver.
> `transaction` opens the transaction with `TransactionTrait::begin`,
> `transaction_with` with `begin_with(opts)`; the rest of the cycle is shared.
> `Ok(value)` is followed by `COMMIT`, and `value` is returned only once the
> `COMMIT` succeeded. `Err(e)` is followed by an *awaited* `rollback()`, so the
> transaction is over by the time the caller is resumed — this is the explicit
> path of `conn.tx.guard`, not the fire-and-forget `Drop` path, and it is why
> the closure API does not inherit that rule's unobservable-rollback caveat.
>
> The `&'s mut self` borrow is MOVED into the opening call
> (`TransactionTrait::begin(self)` / `DatabaseConnection::begin_with(self,
> opts)`) rather than reborrowed, so the closure is handed the concrete
> `DatabaseTransaction<'s>` and `F` needs no higher-ranked bound over the
> transaction's own lifetime.
>
> Failures are wrapped in `TransactionError<E>`, whose two variants keep the two
> kinds of failure apart: `Connection(DbErr)` for a failing `BEGIN` or `COMMIT`
> — the transaction machinery — and `Transaction(E)` for the closure's own
> error. It is `Debug` unconditionally, `Display` where `E: Display`, and
> `std::error::Error` with a `source()` where `E: Error + 'static`. There is
> deliberately no `From<DbErr> for TransactionError<E>`: with `E = DbErr`, the
> common case, it would file every closure-side error under `Connection` and
> erase the distinction the enum exists to draw.
>
> When the closure returned an error AND the `ROLLBACK` then fails, the closure
> error is what the caller receives; the rollback failure is reported through
> `tracing::error!` and not substituted. The closure error is the cause and the
> failed rollback its consequence, so promoting the latter would replace the
> answer to "why did this transaction fail?" with a symptom.

> [spec:pgorm:sem:conn.tx.retry]
> `DatabaseConnection::transaction_with_retry(opts, max_retries, f)` is
> `transaction_with` plus replay: it retries the whole begin/run/commit cycle —
> not the failing statement — while the failure is retryable, for at most `1 +
> max_retries` attempts. Each attempt reborrows the connection for a fresh
> `begin_with(opts)`, so unlike `conn.tx.closure` the bound is higher-ranked
> over the transaction lifetime: `F: AsyncFnMut(&mut DatabaseTransaction<'_>) ->
> Result<T, E>`. A failed attempt is rolled back exactly as `conn.tx.closure`
> specifies before the next one begins.
>
> Retryable means SQLSTATE `40001` (`serialization_failure`) or `40P01`
> (`deadlock_detected`) — the transaction-rollback errors PostgreSQL raises
> expecting the client to replay — as decided by `DbErr::is_retryable()`, which
> is `false` for every non-`DbErr::Postgres` variant and for any `Postgres`
> error carrying no `DbError`. Anything else returns immediately, on the first
> attempt, with its variant intact.
>
> Both failure sites are classified, because a serialization failure can surface
> either mid-transaction (a statement raises it, reaching the helper as the
> closure's `E`) or at `COMMIT` (reaching it as a `DbErr`). Classifying the
> closure's error requires seeing inside an otherwise opaque `E`, so
> `transaction_with_retry` bounds `E: RetryableError` — a single-method trait
> (`is_retryable(&self) -> bool`) implemented for `DbErr` and implementable for
> a domain error type. `transaction` and `transaction_with` carry no such bound.
>
> `F` is `AsyncFnMut` rather than `AsyncFnOnce` because it is called once per
> attempt, which makes replayability the caller's obligation: work the closure
> does inside the transaction is undone by the rollback, but any effect outside
> it happens again on every attempt.
