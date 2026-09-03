# Metrics Layer

`src/metric.rs` provides an opt-in instrumentation layer. The core types
(`DatabasePool`, `DatabaseConnection`, `DatabaseTransaction`) carry no metrics
hooks at all; instrumentation exists only in wrapper types the application
chooses to construct, so unused metrics cost nothing.

## Collector contract

> [spec:pgorm:def:metric.layer+1]
> The metrics layer consists of a `MetricsCollector` trait (`Clone + Send +
> Sync + 'static`) and three wrappers parameterized by it:
> `InstrumentedPool<M>` (wrapping `DatabasePool`), `InstrumentedConnection<M>`
> (wrapping `DatabaseConnection`), and `InstrumentedTransaction<'a, M>`
> (wrapping `DatabaseTransaction<'a>`). Each wrapper exposes `inner()` and
> `metrics()` accessors to reach the wrapped value and the collector; the pool
> wrapper also forwards `tag()` and `status()`. Two further public types are
> the argument the query hooks take rather than machinery of their own:
> `QueryContext<'a>` and `QueryFingerprint`, both defined by
> `metric.fingerprint`.

> [spec:pgorm:def:metric.layer.collector+1]
> `MetricsCollector` defines seven async hook points:
> `record_query_success(query, duration, rows)`,
> `record_query_error(query, duration, error)`,
> `record_connection_acquired(duration)`,
> `record_connection_error(duration, error)`,
> `record_transaction_begin(duration)`,
> `record_transaction_commit(duration)`, and
> `record_transaction_rollback(duration)`.
>
> The two query hooks take a `QueryContext<'_>` rather than a bare operation
> name; the operation is one of its fields, so identifying a statement more
> precisely is a question of what the context carries rather than of the
> trait's arity. The trait keeps no default methods, so every implementor
> writes all seven; a hook it does not care about is an empty body.
>
> Two implementations ship in-tree: `NoOpMetrics`, whose hooks are all empty
> bodies, and `LoggingMetrics`, which emits `tracing` events — `debug` for
> query success, connection acquired, transaction begin, and commit; `warn`
> for query errors and rollbacks; `error` for connection failures. Its two
> query messages end with ` [<fingerprint>]` when the statement has one, and
> carry no suffix at all when it does not.

## Query identity

> [spec:pgorm:req:metric.fingerprint]
> `QueryContext` is what a query hook is told about the statement it reports
> on: `operation()` — one of the seven `ConnectionTrait` method names, or
> `"begin"` / `"rollback"` for a failed transaction round trip —, `sql()` (the
> statement text, where it survives per `conn.sql-text`), and `fingerprint()`.
> It borrows for the duration of the call, so a collector that keeps anything
> copies it out. A `QueryFingerprint` is libpg_query's constants-normalized
> parse-tree hash: `Display` renders its canonical 16-character zero-padded
> hex, `value()` hands back the same number as a `u64`, and it is the notion of
> query identity `pg_stat_statements` aggregates by. Statements differing only
> in their literals or their whitespace share one; statements differing in
> shape do not.
>
> `fingerprint()` MUST NOT fail the query path, and so returns an `Option`
> whose `None` covers three cases without distinguishing them: the
> off-by-default `metrics-fingerprint` feature is not enabled, so no parser is
> linked in; the statement carries no text to parse; or libpg_query rejected
> the text it was given. The last is not an error — raw SQL the server accepts
> may still be text this parser will not reduce to a tree. Such a statement
> executes normally and is reported normally; only its identity is missing. An
> empty statement, by contrast, is an empty parse rather than a rejected one,
> and does have a fingerprint.
>
> Fingerprints are computed on demand rather than when the context is built, so
> a collector that never asks pays only for the two borrowed fields.
> Computing one is a parse, so answers are memoized process-wide by statement
> text — a rejection memoized as readily as a fingerprint — in an
> `RwLock<HashMap>` bounded at 1024 distinct texts. At the bound the memo stops
> admitting new entries rather than evicting: an application's statement shapes
> are a fixed set well under it, and the texts that would overrun it are the
> per-call ones (an `IN` list whose arity follows the input, a generated
> script), which are better re-parsed than retained forever.
>
> Without the feature pgorm gains no dependency at all, and the public shape is
> unchanged: both types still exist, the hooks still take a context, and
> `fingerprint()` is simply always `None`. A collector therefore compiles
> against either build, and enabling the feature changes no API — only an
> answer.

## Delegation contract

> [spec:pgorm:req:metric.layer.delegate+4]
> Instrumented wrappers MUST delegate every operation to the wrapped type and
> return its result unchanged — wrapping preserves `ConnectionTrait`
> semantics, adding only timing and collector calls around each awaited
> operation. Both `InstrumentedConnection` and `InstrumentedTransaction`
> implement `ConnectionTrait`; on success they report
> `record_query_success` with a `QueryContext` (`metric.fingerprint`) naming
> the operation (`"execute"`,
> `"execute_raw"`, `"query_one"`, `"query_opt"`, `"query_all"`,
> `"query_raw"`, `"batch_execute"`) and carrying the statement's own text —
> the `&str`/`&String` argument, or `sql` for `batch_execute` — and a row
> count — the affected-row count for `execute`/`execute_raw`, `1` for
> `query_one`, `1`/`0` for `query_opt` `Some`/`None`, `rows.len()` for
> `query_all`, `None` for `query_raw` per `exec.stream.decode`, and `None`
> for `batch_execute`, which yields no rows at all — and on
> failure they report `record_query_error` with the same
> context before propagating the `Error`. `InstrumentedPool::get`
> times pool acquisition, reporting `record_connection_acquired` on success
> (and returning an `InstrumentedConnection` sharing a clone of the
> collector) or `record_connection_error` on failure.

## Transaction instrumentation

> [spec:pgorm:sem:metric.layer.tx+2]
> `TransactionTrait::begin` on `InstrumentedConnection` times `BEGIN` and
> reports `record_transaction_begin` on success; a failed begin is reported
> through `record_query_error` under the operation `"begin"` — a context with
> no statement text, since none was sent — not a dedicated hook. Its return
> type is fixed by the trait, so it hands back a plain `DatabaseTransaction`:
> statements issued through that handle bypass the collector entirely unless
> the caller wraps it via `InstrumentedTransaction::new`.
> `InstrumentedConnection::begin_instrumented` is the wrapping counterpart — an
> inherent method reporting the same two hooks, but returning an
> `InstrumentedTransaction<'_, M>` sharing a clone of the collector, so
> per-statement metrics inside the transaction need no second call.
>
> `InstrumentedTransaction::commit` times the commit and reports
> `record_transaction_commit` on success; a failed commit is reported as
> `record_transaction_rollback` (Postgres aborts the transaction when commit
> fails). `InstrumentedTransaction::rollback` consumes the handle, awaits the
> inner `DatabaseTransaction::rollback`, and reports
> `record_transaction_rollback` on either outcome — the transaction is
> discarded whether or not the `ROLLBACK` round trip succeeds — additionally
> reporting a failed round trip through `record_query_error` under the
> operation `"rollback"`, likewise with no statement text.
>
> Its `Drop` impl is an empty no-op, and dropping an uncommitted instrumented
> transaction therefore records nothing at all — not even a rollback. This is a
> limit, not a policy: every collector hook is `async` while `Drop::drop` is
> synchronous, so no hook is reachable from drop. Drop defers entirely to the
> inner `DatabaseTransaction`'s drop behavior (tracing warning plus a
> fire-and-forget `ROLLBACK`), and a rollback only reaches the collector when a
> caller asks for one by calling `rollback` (or when a failing `commit` forces
> one).
