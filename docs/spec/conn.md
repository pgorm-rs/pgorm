# Connections, Pooling, and Transactions

pgorm's connection layer is a thin composition over a vendored deadpool-postgres
fork (`pgorm-pool`) and `tokio-postgres`. `src/database/` exposes the pool and
connection handles plus the `ConnectionTrait` / `TransactionTrait` surface;
`pgorm-pool/src/` owns connection lifecycle, recycling, and statement caching.

## Pool construction and access

> [spec:pgorm:req:conn.pool+3]
> `connect_with(config, tls, manager, build)` is the general pool constructor,
> and the only one: every other entry point delegates to it. It takes the
> `tokio_postgres::Config`, a TLS connector bounded exactly as
> `pgorm_pool::Manager::from_config` bounds its own (`T: MakeTlsConnect<Socket>
> + Clone + Sync + Send + 'static`, plus `Send + Sync` on `T::Stream` and
> `T::TlsConnect` and `Send` on the connect future), the whole `ManagerConfig`,
> and a `FnOnce(PoolBuilder) -> PoolBuilder`. It builds the `Manager` from the
> first three, applies `build` to the resulting `PoolBuilder`, and returns
> `Result<DatabasePool, Error>`.
>
> It exists because `DatabasePool` wraps a crate-private `pgorm_pool::Pool` and
> has no other constructor, which made two things unreachable rather than merely
> inconvenient. `Pool` is TLS-erased (`Box<dyn Connect>`), so a TLS pool was
> always buildable inside `pgorm-pool` and never assignable to a `DatabasePool`
> — leaving every managed PostgreSQL that requires TLS unusable through pgorm.
> And the `Manager` was fully built before any caller hook could run, so
> `RecyclingMethod::Verified`/`Clean`/`Custom` and
> `StatementCacheSize::Bounded`/`Disabled` had no reachable setter at all:
> `ManagerConfig` is private once the `Manager` holds it, and `PoolBuilder`
> reaches only the pool. There is deliberately no `From<pgorm_pool::Pool> for
> DatabasePool` beside it — construction stays funnelled through one function,
> so a `DatabasePool` always holds a pool pgorm shaped.
>
> `ManagerConfig`, `RecyclingMethod`, `StatementCacheSize` and `PoolBuilder`,
> together with `NoTls`, `Socket`, `MakeTlsConnect` and `TlsConnect`, MUST be
> re-exported from `pgorm`. Every one of them is named in `connect_with`'s
> signature or its bounds, and calling it should not require depending on
> `pgorm-pool` or `tokio-postgres` directly.
>
> `connect(config: tokio_postgres::Config) -> DatabasePool` MUST delegate to
> `connect_with` with `NoTls`, `RecyclingMethod::Fast`, no tag, the default
> `StatementCacheSize`, and an identity builder closure — leaving the default
> deadpool pool configuration. Its signature is infallible by design, not by
> omission: `config` shapes the `Manager` and no caller input reaches the pool
> builder, so the only way the build can fail is if pgorm's own defaults are
> invalid. That is an internal invariant and MUST panic.
>
> `connect_with_builder(config, build)` MUST delegate with that same `NoTls` and
> those same manager defaults, applying the caller's closure to the
> `PoolBuilder` before building, and MUST return `Result<DatabasePool, Error>`.
> Because the closure is caller input, a builder shaped into an unbuildable pool
> — deadpool rejects timeouts configured without a runtime — MUST surface as
> `Error::Custom` carrying the builder's message, never as a panic. That is the
> rule for every fallible entry point here: `connect_with` itself, and
> `connect_multi_with_builder` (`conn.pool.multi`), fail the same way.

> [spec:pgorm:sem:conn.pool.get+1]
> `DatabasePool::get()` asynchronously acquires a pooled connection and wraps
> it as `DatabaseConnection`; acquisition failure surfaces as
> `Error::Pool(pgorm_pool::PoolError)` via `From`. `DatabasePool::status()`
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

> [spec:pgorm:sem:conn.pool.multi+2]
> `connect_multi_with_builder(config, builders)` builds one pool per entry of a
> `BTreeMap<String, builder-fn>`: each pool clones the shared
> `tokio_postgres::Config`, is tagged with its map key, and uses
> `RecyclingMethod::Fast`; the result is a
> `Result<BTreeMap<Arc<String>, DatabasePool>, Error>` keyed by tag. Every
> builder is caller input, so construction is fallible on the same terms as
> `connect_with_builder` (`conn.pool`): the first entry whose builder cannot
> produce a pool aborts the whole construction with `Error::Custom` and no map
> is returned. `DatabaseMultiPool` wraps the same map shape and offers
> `get(key) -> Option<DatabasePool>` (a cheap clone of the refcounted pool) and
> `status()` returning per-tag `deadpool::Status`.
>
> Selection is explicit by tag key only — there is no round-robin, load
> balancing, or fallback across pools. `DatabaseMultiPool` currently has no
> public constructor (its field is crate-private and
> `connect_multi_with_builder` returns the raw `BTreeMap` on success), so
> outside the crate it is effectively read-only API surface.

> [spec:pgorm:req:conn.pool.config-forwarding]
> `pgorm_pool::Config` is the deserializable connection configuration — the
> shape `PG__HOST`, `PG__PASSWORD`, `PG__SSL_MODE` and their siblings land in.
> Every field it declares MUST reach the `tokio_postgres::Config` that
> `get_pg_config` builds, or else MUST NOT be declared. A field the struct
> accepts and the connection never sees is a setting the caller wrote and the
> database was never told about, and nothing anywhere reports the gap.
>
> Three fields were declared, given `From` conversions to their tokio-postgres
> counterparts, and then never applied: `target_session_attrs`,
> `channel_binding`, and `load_balance_hosts`. The conversions were dead code,
> and the settings were dropped in every spelling but one — a value embedded in
> `url` survived, because `tokio_postgres::Config::from_str` parses it — so the
> same setting was honoured or ignored depending on which way it was written.
>
> Two of the three lose more than configuration. `channel_binding: Require`
> dropped downgrades the connection to whatever binding the server will settle
> for, which is the opposite of what `Require` asks and is invisible.
> `target_session_attrs: ReadWrite` dropped lets a connection land on a hot
> standby, where the first write fails with SQLSTATE `25006` at run time — this
> setting exists precisely to move that failure to connect time.
>
> `ssl_mode` was already forwarded and stays so. The rule is over the whole
> struct, not over the three that happened to be missing.

> [spec:pgorm:sem:conn.pool.config-redaction]
> `Debug` on a pool configuration type MUST NOT reveal a credential.
> `pgorm_pool::Config` carries two. `password` is one, and the crate's own
> documentation tells the reader to fill it from `PG__PASSWORD`; `url` is the
> other, since a connection string's `userinfo` holds the same secret in a
> different spelling. A derived `Debug` printed both verbatim, so a single
> `tracing::info!(?cfg)` at startup put the database password in the logs.
>
> The impl is therefore hand-written. `password` prints as `_` when set and
> `None` when not — `tokio_postgres::Config`'s own redaction spelling, and the
> reason pgorm's `DatabasePool` Debug chain was already safe. `url` prints with
> its `userinfo` replaced by `_`, keeping the host, port, database and options
> after it: those are what make the value worth printing, and only the part
> before the `@` is secret. A `url` in libpq keyword/value form has no `@` to
> cut at, and its quoting is not worth re-implementing to find the boundary, so
> one that mentions `password` at all is withheld whole as `_`.
>
> `Serialize` is deliberately unchanged. Round-tripping the configuration is
> what it is for, and a serializer that dropped the password would emit a config
> that cannot connect; the two traits answer different questions, and only one
> of them answers into a log.

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

> [spec:pgorm:sem:conn.pool.statement-cache+2]
> Each `ClientWrapper` carries an `Arc<StatementCache>` keyed by `(query text,
> parameter types)`. `prepare_cached` / `prepare_typed_cached` return the
> cached `tokio_postgres::Statement` on hit and prepare-then-insert on miss.
> `Transaction`s, nested transactions, and savepoints created through the
> wrapper share the owning client's cache. The manager-level `StatementCaches`
> holds weak references to every live cache and supports `clear()` and
> `remove(query, types)` across all pooled connections.
>
> The cache is on the ordinary execution path, not beside it, and it is the
> whole of that path: each of `ConnectionTrait`'s six statement methods
> resolves its SQL through the cache (`conn.pool.conn-trait`), with no
> unresolved alternative beside it, so one text is parsed once per connection
> rather than once per call. What still bypasses it is `tokio_postgres`'s own
> statement surface — `Client::query`, `Transaction::execute` and their
> siblings prepare an unnamed statement per call and consult nothing — which is
> what pgorm-pool's `Client`/`Transaction` expose by `Deref` and what a caller
> reaching past `ConnectionTrait` gets.
>
> A cached statement outlives the call that prepared it. That is the point, and
> two things follow from it: the cache is capacity-bounded
> (`conn.pool.statement-cache.bound`), because the key space is not, and a
> cached plan can be invalidated by DDL under it
> (`conn.pool.statement-cache.invalidate`).

> [spec:pgorm:req:conn.pool.statement-cache.bound+1]
> A `StatementCache` MUST be bounded. Its key space is the SQL text, and one
> logical query spreads across many texts — an `IN` list rendered with a
> placeholder per element is a different text at every arity, 25 of them for one
> measured query — while every entry is a live server-side prepared statement
> that `Fast` and `Clean` recycling both deliberately keep
> (`conn.pool.recycle`). Unbounded, the cache would hold statements the
> connection will never run again for as long as the connection lives.
>
> `ManagerConfig::statement_cache: StatementCacheSize` carries the bound, per
> connection: `Bounded(NonZeroUsize)`, defaulting to 256, or `Disabled`. There
> is deliberately no unbounded variant — that is the growth the type exists to
> stop, and a caller wanting an effectively unlimited cache says so with a large
> `Bounded` — and no zero bound, which would be `Disabled` spelled a second way.
> The default is an order of magnitude above the worst measured spread of a
> single query and keeps even a large pool's server-side statement count in the
> low thousands.
>
> Inserting into a full cache MUST evict an existing entry before inserting, and
> MUST NOT panic or refuse. The victim is whichever entry the map yields first,
> not the least recently used: recency would have to be written on every lookup,
> making the read lock a cache hit takes today an exclusive one, and the bound
> exists to stop growth rather than to maximise the hit rate. Re-inserting a key
> the cache already holds replaces it and evicts nothing.
>
> `Disabled` stores and looks up nothing, so `prepare_cached` is
> `prepare_typed` itself: every call parses its own statement and closes it when
> the last handle drops, which is the behaviour that predates the routing of
> `conn.pool.conn-trait`. It is the opt-out for a caller who would rather pay
> the parse than reason about a plan cached across DDL.
>
> Evicting a statement — like disabling the cache, and like recycling with SQL
> that deallocates — drops pgorm's last handle to it only once the caller has
> dropped the rows it produced, because a `tokio_postgres::Row` holds the
> statement it was decoded against. The `Close` reaches the server when that
> happens, not when the entry leaves the map.
>
> pgorm's `connect`, `connect_with_builder` and `connect_multi_with_builder`
> (`conn.pool`, `conn.pool.multi`) always take the default. The knob is reached
> through `connect_with`, the general entry point those three delegate to, which
> takes the whole `ManagerConfig`.

> [spec:pgorm:req:conn.pool.statement-cache.invalidate+3]
> Reusing a prepared statement admits one failure that preparing afresh does
> not: PostgreSQL raises SQLSTATE `0A000` — *cached plan must not change result
> type* — when a statement's plan is revalidated and the result it would now
> produce no longer matches the description the client was given. DDL under a
> live cache entry is how that happens: `SELECT *` over a table that gained a
> column, or a projection whose column changed type. Three neighbouring cases
> were probed and are NOT hazards. PostgreSQL re-plans two of them itself:
> changing `search_path` so the same text resolves to a different table, and
> dropping and recreating the table a cached statement names. The third is a
> statement first prepared inside a transaction that then rolls back — the cache
> is the connection's, shared with every transaction on it (`conn.tx`), so this
> would leave an entry naming a statement the rollback had discarded. It does
> not: a statement parsed over the extended protocol is not undone by
> `ROLLBACK`, and the cached entry keeps working.
>
> When a statement resolved through the cache fails with `0A000`, pgorm MUST
> evict that key. Whether it then retries depends on whether a transaction is
> open, because `0A000` is an error and an error aborts an open transaction.
>
> Outside a transaction, pgorm MUST re-prepare the evicted key exactly once and
> execute again. A second `0A000` MUST reach the caller as the `Error::Postgres`
> it is: the recovery is a single retry, never a loop, because a plan that is
> stale twice running is not a plan going stale.
>
> Inside a transaction, pgorm MUST NOT retry, and MUST surface the `0A000`
> unchanged. The transaction is already aborted by the time the error arrives,
> so the re-prepare fails on its own account with SQLSTATE `25P02`, *current
> transaction is aborted*, and it is that error the caller would be handed: the
> one fact they can act on, displaced by a consequence of pgorm's own retry.
> More generally, no recovery pgorm attempts may substitute its own failure for
> the failure it was recovering from. The eviction still happens, so the
> caller's next attempt prepares afresh, and the recovery is theirs to run at
> the boundary they own — retry the transaction (`conn.tx.retry`), or roll back
> to a savepoint opened as a nested transaction (`conn.tx`) and re-run inside
> the still-live outer one.
>
> Retrying under an implicit savepoint is rejected rather than merely
> unimplemented. It is the only shape recovery inside a transaction could take,
> since an abort cannot be undone after the fact and the savepoint would
> therefore have to be taken before every cached execution rather than on
> failure — two extra round trips on every in-transaction statement, to hide a
> DDL race, spent by machinery whose purpose (`conn.pool.statement-cache`) is to
> remove round trips. It would also duplicate a facility callers already reach
> explicitly: a nested transaction is a savepoint (`conn.tx`), so whoever wants a
> statement fenced can fence it, and whoever does not is not charged for it.
>
> Which of the two applies is decided by the type, not by asking the server.
> `ConnectionTrait` on `DatabaseConnection` and `&DatabaseConnection` is the
> retrying path, and on `DatabaseTransaction` — with `&mut DatabaseTransaction`
> forwarding to it (`conn.pool.conn-trait`) — the evict-only one. No third case
> hides between them: `TransactionTrait::begin` borrows the connection
> exclusively (`conn.tx`), so while a transaction handle is alive the borrow
> checker forbids reaching the connection's impls at all.
>
> `execute_raw` and `query_raw` never retry, transaction or no transaction. They
> take an `IntoIterator` consumed by the first attempt, which is not `Clone` and
> cannot be held across the retry without a `Send` bound `ConnectionTrait` does
> not carry, so they evict the rejected key and return the error — which leaves
> the next call to re-prepare, making the recovery one call later rather than
> absent. The four methods whose parameters are a reusable `&[&(dyn ToSql +
> Sync)]` slice are the ones that *can* retry, and outside a transaction they
> do. Eviction itself has no exception: every statement `ConnectionTrait`
> accepts is text (`conn.sql-text`), so every one is cache-resolved and every
> one can be prepared again from what the caller passed.
>
> `0A000` is also PostgreSQL's generic *feature not supported*, and nothing but
> the message text — which is localized — separates the two. A statement
> rejected on its own merits is therefore treated the same way: outside a
> transaction it is retried once and fails identically, costing one round trip
> on a call that was already failing; inside one it costs an eviction, so the
> next call parses the text again. That is preferred to matching on prose, and
> it is also why `0A000` is NOT added to the retryable SQLSTATEs of
> `conn.tx.retry` — `transaction_with_retry` would replay whole transactions
> against a permanently unsupported feature. A caller who knows their statements
> reach no such feature can classify it as retryable themselves, since
> `RetryableError` is implemented on the closure's own error type.
>
> One case is out of reach of this rule rather than absent from it. A recycling
> method whose SQL deallocates — `Custom("DISCARD ALL")`, which is exactly what
> `Clean` avoids (`conn.pool.recycle`) — drops every server-side statement while
> the cache keeps naming them, and the next use fails with SQLSTATE `26000`,
> which is not retried. `connect` and its two sibling shorthands recycle with
> `Fast` (`conn.pool`), so they cannot reach it; a caller who passes such a
> method to `connect_with` should pair it with `StatementCacheSize::Disabled`.

## Statement execution surface

> [spec:pgorm:def:conn.sql-text+2]
> `SqlText` answers `fn sql_text(&self) -> &str` for a statement, and is what
> `ConnectionTrait` means by one. It MUST be sealed against implementations
> outside pgorm, and MUST be implemented for exactly `str` and `String` — a
> statement is its SQL text, in whichever of the two spellings the caller holds
> it, and there is deliberately no third form.
>
> What the seal excludes is tokio-postgres's prepared `Statement`.
> `ToStatement`, which bounds tokio-postgres's own statement surface, admits
> `str`, `String` and `Statement` alike; but a `Statement` names a statement
> that exists on the single connection it was prepared against, so running one
> on any other connection type-checks and then fails on the wire with SQLSTATE
> `26000`, *prepared statement does not exist*. Under a pool, which connection
> a call runs on is not the caller's to choose, so a bound that accepts a
> `Statement` is a bound that accepts a statement bound to a connection nobody
> named. Removing the type from the bound removes the call rather than
> diagnosing it (`[dec:pgorm:invalid-states-unrepresentable]`), and costs
> nothing pgorm could have used: a `Statement` retains its server-side name,
> parameter types and result columns but not the text it was prepared from, so
> it is exactly the statement pgorm can neither cache nor fingerprint.
>
> Because a statement is nothing but text, `sql_text` is total — no `Option`,
> and no absent case for its readers to carry. There are two:
> `conn.pool.conn-trait` keys the statement cache on it, and
> `metric.fingerprint` identifies the query it reports on by it.
>
> The narrowing is `ConnectionTrait`'s alone. `pgorm_pool`'s `GenericClient`
> (`conn.pool.generic-client`) and the `tokio_postgres` surface that pgorm-pool's
> `Client` and `Transaction` expose by `Deref` keep `ToStatement`: those are
> tokio-postgres's own contract, reached past `ConnectionTrait` with a
> particular connection already in hand, and there a prepared `Statement` is
> the point rather than the hazard.

> [spec:pgorm:def:conn.pool.conn-trait+8]
> `ConnectionTrait` is the uniform statement-execution surface over
> connections and transactions. It defines seven async methods. Six are
> generic over `T: ?Sized + SqlText + Sync` — the statement is SQL text and
> nothing else (`conn.sql-text`), which is why `ToStatement` is named nowhere
> in this trait and a `Statement` prepared on some other connection does not
> typecheck — with parameter binding
> (no string interpolation): `execute(stmt, params) -> u64` (affected-row
> count), `execute_raw(stmt, params)` taking an `ExactSizeIterator` of
> `BorrowToSql` values instead of a `&[&dyn ToSql]` slice, `query_one ->
> Row` (errors if not exactly one row), `query_opt -> Option<Row>`,
> `query_all -> Vec<Row>`, and `query_raw(stmt, params) -> RowStream`,
> which takes the same `BorrowToSql` iterator as `execute_raw` and returns
> the unbuffered row stream of `exec.stream`. Errors map to
> `Error::Postgres`.
>
> All six resolve the statement through the connection's `StatementCache`
> (`conn.pool.statement-cache`) before executing it, so a text executed twice on
> one connection is parsed once: the text is looked up, and prepared and
> inserted on a miss. There is no second route, because there is no statement
> without text to take one. What the cache returns is prepared on, and used
> only on, the connection the call is running on, so no prepared statement
> crosses connections at all. A rejected cached plan is evicted under
> `conn.pool.statement-cache.invalidate`, and retried there only outside a
> transaction: inside one the rejection has already aborted the transaction, so
> the original error is what the caller gets.
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
> `DatabaseTransaction`, each resolving through the cache its underlying
> `pgorm-pool` client or transaction owns and then delegating to it — the three
> share one cache per physical connection, since a transaction's is its
> client's (`conn.tx`) — with `batch_execute` going through
> `GenericClient::batch_execute` (`conn.pool.generic-client`), which is
> otherwise unreachable from pgorm because the wrapper types' inner fields are
> crate-private.
>
> A fourth implementor carries no execution of its own: `&mut
> DatabaseTransaction<'_>` forwards every method through the exclusive borrow.
> It exists because that is the handle a transaction closure is given
> (`conn.tx.closure`), and a helper generic over `C: ConnectionTrait` taking
> `&C` cannot be passed one without it — `&mut T` is its own type, distinct
> from `T`, and the coercion to `&T` does not fire into an inference variable,
> so the call site would otherwise have to reborrow as `&*txn`. Being a
> distinct type is also why the impl cannot overlap the one for
> `DatabaseTransaction`.

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

> [spec:pgorm:req:conn.tx+2]
> `TransactionTrait::begin(&mut self)` MUST return a `DatabaseTransaction<'_>`
> borrowing the parent exclusively, so no other statement can run on the
> connection while the transaction handle is alive. `DatabaseConnection::begin`
> issues `BEGIN` on the pooled client; `DatabaseTransaction::begin` creates a
> nested transaction, which `tokio_postgres` implements as a savepoint. Both
> transaction levels share the connection's statement cache.
>
> `DatabaseConnection::begin_with(mode: TransactionMode)` is the configured
> counterpart, opening the transaction through `pgorm_pool`'s
> `TransactionBuilder`, which emits `START TRANSACTION` followed by the clauses
> the mode selects. `TransactionMode` MUST be a closed `Debug + Clone + Copy +
> Default` enum of exactly four variants, each one a combination PostgreSQL
> acts on:
>
> - `Default` — the `Default::default()` variant — appends no clause. The bare
>   `START TRANSACTION` inherits `default_transaction_isolation`,
>   `default_transaction_read_only`, and `default_transaction_deferrable`, so it
>   opens the same transaction `begin` does.
> - `ReadWrite { isolation: Option<IsolationLevel> }` appends `ISOLATION LEVEL
>   <level>` when `isolation` is `Some`, then `READ WRITE`. That clause is
>   emitted unconditionally rather than elided as the usual server default, so
>   the mode overrides a session running under `SET
>   default_transaction_read_only = on` instead of inheriting it.
> - `ReadOnly { isolation: Option<IsolationLevel> }` appends `ISOLATION LEVEL
>   <level>` when `isolation` is `Some`, then `READ ONLY`. A write inside such a
>   transaction is rejected by the server with SQLSTATE `25006`
>   (`read_only_sql_transaction`).
> - `DeferrableSnapshot` appends `ISOLATION LEVEL SERIALIZABLE, READ ONLY,
>   DEFERRABLE`, and takes no parameter. Opening it may block until the snapshot
>   is safe, after which the transaction cannot be aborted by a serialization
>   failure.
>
> `DeferrableSnapshot` MUST be the only shape that reaches `DEFERRABLE`, and it
> MUST carry the isolation level and access mode itself rather than accepting
> either from the caller. PostgreSQL honours `DEFERRABLE` only when the
> transaction is both `SERIALIZABLE` and `READ ONLY`; in every other
> combination the server parses the clause and ignores it. Deferrability as an
> independent flag would therefore be constructible, accepted, and a silent
> no-op — an invalid state the type MUST NOT be able to represent. For the same
> reason no variant offers `NOT DEFERRABLE`: it is the server default, and
> selecting it says nothing.
>
> `IsolationLevel` is `tokio_postgres::IsolationLevel`, re-exported from
> `pgorm`.
>
> `begin_with` is an inherent method on `DatabaseConnection`, deliberately NOT
> on `TransactionTrait`: the trait's other implementor is
> `DatabaseTransaction`, whose `begin` is a savepoint, and `SAVEPOINT` accepts
> no isolation level, access mode, or deferrability. Putting the configured
> form on the trait would require the savepoint implementation to ignore or
> reject its argument, so it is offered only where PostgreSQL honours it.

> [spec:pgorm:sem:conn.tx.guard+2]
> `DatabaseTransaction` wraps `Option<pgorm_pool::Transaction>` as a
> commit-or-rollback guard. `commit(self)` and `rollback(self)` each consume
> the handle, `take()` the inner transaction, and await `COMMIT` / `ROLLBACK`
> respectively, mapping failure to `Error::Postgres`. Because both take the
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

> [spec:pgorm:sem:conn.tx.closure+2]
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
> The handle the closure receives is a `ConnectionTrait` implementor in its own
> right (`conn.pool.conn-trait`), so `&txn` is what a helper generic over `&C`
> is passed, inside a closure exactly as outside one; no `&*txn` reborrow is
> required at the call site.
>
> `E` stays the caller's. Where a closure body's tail is a bare `Ok(value)` and
> every fallible call reaches `?`, nothing pins `E` and the caller annotates —
> `Ok::<_, Error>(value)`. There is deliberately no fixed-`Error` sibling of
> these methods to spare that annotation: it would be a second name per entry
> point differing only in a type parameter, and it would have to either return
> the same `TransactionError<Error>` — an inference alias, nothing more — or
> flatten to `Error` and erase the distinction `TransactionError` exists to
> draw.
>
> Failures are wrapped in `TransactionError<E>`, whose two variants keep the two
> kinds of failure apart: `Connection(Error)` for a failing `BEGIN` or `COMMIT`
> — the transaction machinery — and `Transaction(E)` for the closure's own
> error. It is `Debug` unconditionally, `Display` where `E: Display`, and
> `std::error::Error` with a `source()` where `E: Error + 'static`. There is
> deliberately no `From<Error> for TransactionError<E>`: with `E = Error`, the
> common case, it would file every closure-side error under `Connection` and
> erase the distinction the enum exists to draw.
>
> When the closure returned an error AND the `ROLLBACK` then fails, the closure
> error is what the caller receives; the rollback failure is reported through
> `tracing::error!` and not substituted. The closure error is the cause and the
> failed rollback its consequence, so promoting the latter would replace the
> answer to "why did this transaction fail?" with a symptom.

> [spec:pgorm:sem:conn.tx.retry+1]
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
> expecting the client to replay — as decided by `Error::is_retryable()`, which
> is `false` for every non-`Error::Postgres` variant and for any `Postgres`
> error carrying no `DbError`. Anything else returns immediately, on the first
> attempt, with its variant intact.
>
> Both failure sites are classified, because a serialization failure can surface
> either mid-transaction (a statement raises it, reaching the helper as the
> closure's `E`) or at `COMMIT` (reaching it as an `Error`). Classifying the
> closure's error requires seeing inside an otherwise opaque `E`, so
> `transaction_with_retry` bounds `E: RetryableError` — a single-method trait
> (`is_retryable(&self) -> bool`) implemented for `Error` and implementable for
> a domain error type. `transaction` and `transaction_with` carry no such bound.
>
> `F` is `AsyncFnMut` rather than `AsyncFnOnce` because it is called once per
> attempt, which makes replayability the caller's obligation: work the closure
> does inside the transaction is undone by the rollback, but any effect outside
> it happens again on every attempt.
