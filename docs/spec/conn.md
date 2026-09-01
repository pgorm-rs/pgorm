# Connections, Pooling, and Transactions

pgorm's connection layer is a thin composition over a vendored deadpool-postgres
fork (`pgorm-pool`) and `tokio-postgres`. `src/database/` exposes the pool and
connection handles plus the `ConnectionTrait` / `TransactionTrait` surface;
`pgorm-pool/src/` owns connection lifecycle, recycling, and statement caching.
(`src/database/stream/` is currently disabled — commented out of
`src/database/mod.rs` — and is deliberately not specified here;
`src/database/statement.rs` is likewise disabled but its seam is specified
under **Statement seam** below.)

## Pool construction and access

> [spec:pgorm:req:conn.pool]
> `connect(config: tokio_postgres::Config) -> DatabasePool` MUST construct a
> deadpool-backed pool wrapping `pgorm_pool::Pool`: a `Manager` built from the
> given config with `NoTls`, `RecyclingMethod::Fast`, and no tag, combined with
> the default deadpool pool configuration. `connect_with_builder` MUST apply the
> caller's closure to the `PoolBuilder` before building, allowing pool sizing
> and timeout customization.
>
> Both constructors `unwrap()` the builder result: pool construction failure is
> a panic, not a `DbErr`. TLS connections are not supported through these
> entry points (`NoTls` is hard-coded); a custom-TLS pool requires assembling a
> `pgorm_pool::Manager` directly.

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

> [spec:pgorm:sem:conn.pool.multi]
> `connect_multi_with_builder(config, builders)` builds one pool per entry of a
> `BTreeMap<String, builder-fn>`: each pool clones the shared
> `tokio_postgres::Config`, is tagged with its map key, and uses
> `RecyclingMethod::Fast`; the result is a `BTreeMap<Arc<String>, DatabasePool>`
> keyed by tag. `DatabaseMultiPool` wraps the same map shape and offers
> `get(key) -> Option<DatabasePool>` (a cheap clone of the refcounted pool) and
> `status()` returning per-tag `deadpool::Status`.
>
> Selection is explicit by tag key only — there is no round-robin, load
> balancing, or fallback across pools. `DatabaseMultiPool` currently has no
> public constructor (its field is crate-private and
> `connect_multi_with_builder` returns the raw `BTreeMap`), so outside the
> crate it is effectively read-only API surface.

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

> [spec:pgorm:def:conn.pool.conn-trait]
> `ConnectionTrait` is the uniform statement-execution surface over
> connections and transactions. It defines five async methods, all generic
> over `T: ?Sized + ToStatement + Send + Sync` with parameter binding (no
> string interpolation): `execute(stmt, params) -> u64` (affected-row count),
> `execute_raw(stmt, params)` taking an `ExactSizeIterator` of
> `BorrowToSql` values instead of a `&[&dyn ToSql]` slice, `query_one ->
> Row` (errors if not exactly one row), `query_opt -> Option<Row>`, and
> `query_all -> Vec<Row>`. Errors map to `DbErr::Postgres`.
>
> It is implemented for `DatabaseConnection`, `&DatabaseConnection`, and
> `DatabaseTransaction`, each delegating directly to the underlying
> `pgorm-pool` client or transaction. Row streaming (`query_raw`) is
> currently disabled (commented out) and not part of the surface.

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

> [spec:pgorm:req:conn.tx]
> `TransactionTrait::begin(&mut self)` MUST return a `DatabaseTransaction<'_>`
> borrowing the parent exclusively, so no other statement can run on the
> connection while the transaction handle is alive. `DatabaseConnection::begin`
> issues `BEGIN` on the pooled client; `DatabaseTransaction::begin` creates a
> nested transaction, which `tokio_postgres` implements as a savepoint. Both
> transaction levels share the connection's statement cache. Isolation level
> and read-only configuration exist only on a private
> `DatabaseConnection::begin_with_config` helper and are not reachable through
> the public trait.

> [spec:pgorm:sem:conn.tx.guard]
> `DatabaseTransaction` wraps `Option<pgorm_pool::Transaction>` as a
> commit-or-rollback guard. `commit(self)` consumes the handle, takes the
> inner transaction, and commits, mapping failure to `DbErr::Postgres`. There
> is no explicit `rollback` method: rollback is achieved by dropping the
> handle uncommitted, in which case the underlying `tokio_postgres`
> transaction rolls back and pgorm's `Drop` impl emits a
> `tracing::warn!("Transaction dropped without committing!")` — the
> uncommitted-transaction check is a runtime warning, not a compile-time or
> panic-level guarantee.

## Statement seam

> [spec:pgorm:def:conn.statement]
> A `Statement` is `{ sql: String, values: Option<Values> }`, with `Value`
> / `Values` re-exported from `pgorm_query`. Constructors:
> `from_string(sql)` yields `values: None`;
> `from_sql_and_values(sql, values)` collects an iterator of `Value` into
> `Values`. `StatementBuilder` is the single-method trait
> `build(&self) -> Statement`, implemented by macro for the `pgorm_query`
> statement types: the query statements (`InsertStatement`,
> `SelectStatement`, `UpdateStatement`, `DeleteStatement`, `WithQuery`)
> build with `pgorm_query::QueryBuilder` and keep the collected parameter
> values; the schema statements (`TableCreate/Drop/Alter/Rename/Truncate`,
> `IndexCreate/Drop`, `ForeignKeyCreate/Drop`) render to SQL only, with
> `values: None`. `Display` for `Statement` splices the values into the SQL
> via `inject_parameters` when present, otherwise prints the raw SQL.
>
> The whole module is currently dormant: `mod statement;` is commented out
> of `src/database/mod.rs`, so none of these types are reachable through
> the public pgorm crate today.

> [spec:pgorm:sem:conn.statement.disabled-types]
> A third macro, `build_type_stmt!`, renders a statement to a plain SQL
> string via `to_string(QueryBuilder)` for the PostgreSQL `TYPE` DDL
> statements — but every invocation of it
> (`pgorm_query::extension::postgres::TypeCreateStatement`,
> `TypeAlterStatement`, `TypeDropStatement`) is commented out pending the
> postgres `TYPE` extension's return to `pgorm_query`. Consequently
> `StatementBuilder` has no impl for CREATE/ALTER/DROP TYPE statements: the
> macro is defined but currently unused.
