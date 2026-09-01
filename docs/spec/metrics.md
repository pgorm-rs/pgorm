# Metrics Layer

`src/metric.rs` provides an opt-in instrumentation layer. The core types
(`DatabasePool`, `DatabaseConnection`, `DatabaseTransaction`) carry no metrics
hooks at all; instrumentation exists only in wrapper types the application
chooses to construct, so unused metrics cost nothing.

## Collector contract

> [spec:pgorm:def:metric.layer]
> The metrics layer consists of a `MetricsCollector` trait (`Clone + Send +
> Sync + 'static`) and three wrappers parameterized by it:
> `InstrumentedPool<M>` (wrapping `DatabasePool`), `InstrumentedConnection<M>`
> (wrapping `DatabaseConnection`), and `InstrumentedTransaction<'a, M>`
> (wrapping `DatabaseTransaction<'a>`). Each wrapper exposes `inner()` and
> `metrics()` accessors to reach the wrapped value and the collector; the pool
> wrapper also forwards `tag()` and `status()`.

> [spec:pgorm:def:metric.layer.collector]
> `MetricsCollector` defines seven async hook points:
> `record_query_success(operation, duration, rows)`,
> `record_query_error(operation, duration, error)`,
> `record_connection_acquired(duration)`,
> `record_connection_error(duration, error)`,
> `record_transaction_begin(duration)`,
> `record_transaction_commit(duration)`, and
> `record_transaction_rollback(duration)`.
>
> Two implementations ship in-tree: `NoOpMetrics`, whose hooks are all empty
> bodies, and `LoggingMetrics`, which emits `tracing` events — `debug` for
> query success, connection acquired, transaction begin, and commit; `warn`
> for query errors and rollbacks; `error` for connection failures.

## Delegation contract

> [spec:pgorm:req:metric.layer.delegate]
> Instrumented wrappers MUST delegate every operation to the wrapped type and
> return its result unchanged — wrapping preserves `ConnectionTrait`
> semantics, adding only timing and collector calls around each awaited
> operation. Both `InstrumentedConnection` and `InstrumentedTransaction`
> implement `ConnectionTrait`; on success they report
> `record_query_success` with the operation name (`"execute"`,
> `"execute_raw"`, `"query_one"`, `"query_opt"`, `"query_all"`) and a row
> count — the affected-row count for `execute`/`execute_raw`, `1` for
> `query_one`, `1`/`0` for `query_opt` `Some`/`None`, and `rows.len()` for
> `query_all` — and on failure they report `record_query_error` with the same
> operation name before propagating the `DbErr`. `InstrumentedPool::get`
> times pool acquisition, reporting `record_connection_acquired` on success
> (and returning an `InstrumentedConnection` sharing a clone of the
> collector) or `record_connection_error` on failure.

## Transaction instrumentation

> [spec:pgorm:sem:metric.layer.tx]
> `TransactionTrait::begin` on `InstrumentedConnection` times `BEGIN` and
> reports `record_transaction_begin` on success; a failed begin is reported
> through `record_query_error("begin", ..)`, not a dedicated hook. The
> returned value is a plain `DatabaseTransaction` — begin does not
> auto-instrument; callers must wrap it via `InstrumentedTransaction::new` to
> keep per-statement metrics inside the transaction.
>
> `InstrumentedTransaction::commit` times the commit and reports
> `record_transaction_commit` on success; a failed commit is reported as
> `record_transaction_rollback` (Postgres aborts the transaction when commit
> fails). Its `Drop` impl is intentionally an empty no-op: dropping an
> uncommitted instrumented transaction records no rollback metric and defers
> entirely to the inner `DatabaseTransaction`'s drop behavior (tracing warning
> plus implicit rollback).
