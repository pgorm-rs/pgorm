# pgorm Observability and Metrics Guide

pgorm ships an opt-in instrumentation layer in `pgorm::metric`. The core types — `DatabasePool`, `DatabaseConnection`, `DatabaseTransaction` — carry no metrics hooks at all; timing and reporting live entirely in wrapper types you choose to construct, so code that never wraps pays nothing. The database itself is still usually the best source of truth for query statistics, so this guide covers both PostgreSQL-native observability and the in-tree layer.

## PostgreSQL-Native Observability (Recommended)

### Database-Level Monitoring

The most efficient approach is to use PostgreSQL's built-in observability features:

#### 1. Query Logging
```sql
-- Log all statements
ALTER SYSTEM SET log_statement = 'all';

-- Log statement durations
ALTER SYSTEM SET log_duration = 'on';

-- Log slow queries only (>1000ms)
ALTER SYSTEM SET log_min_duration_statement = 1000;

-- Reload configuration
SELECT pg_reload_conf();
```

#### 2. pg_stat_statements Extension
```sql
-- Enable the extension (requires restart)
ALTER SYSTEM SET shared_preload_libraries = 'pg_stat_statements';

-- Create the extension
CREATE EXTENSION pg_stat_statements;

-- View query statistics
SELECT 
    query,
    calls,
    total_exec_time,
    mean_exec_time,
    rows
FROM pg_stat_statements 
ORDER BY total_exec_time DESC 
LIMIT 10;
```

#### 3. Connection Pool Status
```rust
use pgorm::connect;

// `connect` returns the pool directly; pool construction failure panics
// rather than surfacing as a `DbErr`.
let pool = connect(config);

// Get pool status (deadpool::Status: max_size, size, available, waiting)
let status = pool.status();
println!("Pool connections - Available: {}, Used: {}", 
    status.available, status.size - status.available);

// Get pool tag (useful for multi-pool setups)
let tag = pool.tag();
println!("Pool: {}", tag);
```

## The `pgorm::metric` Layer

### Collector Contract

`MetricsCollector` is an async trait bounded `Clone + Send + Sync + 'static`, with seven hook points and no default implementations:

```rust
#[async_trait]
pub trait MetricsCollector: Clone + Send + Sync + 'static {
    async fn record_query_success(&self, operation: &str, duration: Duration, rows: Option<u64>);
    async fn record_query_error(&self, operation: &str, duration: Duration, error: &DbErr);
    async fn record_connection_acquired(&self, duration: Duration);
    async fn record_connection_error(&self, duration: Duration, error: &DbErr);
    async fn record_transaction_begin(&self, duration: Duration);
    async fn record_transaction_commit(&self, duration: Duration);
    async fn record_transaction_rollback(&self, duration: Duration);
}
```

Two implementations ship in-tree:

- `NoOpMetrics` — every hook is an empty body.
- `LoggingMetrics` — emits `tracing` events: `debug` for query success, connection acquired, transaction begin, and commit; `warn` for query errors and rollbacks; `error` for connection failures.

### Wrapping a Pool

```rust
use pgorm::ConnectionTrait;
use pgorm::metric::{InstrumentedPool, LoggingMetrics, NoOpMetrics};

let pool = pgorm::connect(config);

// No-op collector: the wrapper still times each operation, but reports nothing.
let quiet = InstrumentedPool::new(pool.clone(), NoOpMetrics);

// Or report every operation through `tracing`.
let instrumented = InstrumentedPool::new(pool, LoggingMetrics);

// Use it like a regular pool.
let conn = instrumented.get().await?;                          // record_connection_acquired
let rows = conn.query_all("SELECT * FROM users", &[]).await?;  // record_query_success("query_all", ..)
```

`InstrumentedPool<M>` forwards `tag()` and `status()` to the wrapped pool and exposes `inner()` / `metrics()` to reach the `DatabasePool` and the collector. `get()` times pool acquisition, reporting `record_connection_acquired` on success — returning an `InstrumentedConnection<M>` that holds a clone of the collector — or `record_connection_error` on failure.

Because `get()` clones the collector for every connection, keep collectors cheap to clone (refcounted handles, not owned state).

### What Gets Recorded

`InstrumentedConnection<M>` and `InstrumentedTransaction<'_, M>` both implement `ConnectionTrait`, delegating each call to the wrapped value and returning its result unchanged. On success:

| Operation      | `operation` string | `rows` reported      |
| -------------- | ------------------ | -------------------- |
| `execute`      | `"execute"`        | affected-row count   |
| `execute_raw`  | `"execute_raw"`    | affected-row count   |
| `query_one`    | `"query_one"`      | `Some(1)`            |
| `query_opt`    | `"query_opt"`      | `Some(1)` / `Some(0)` |
| `query_all`    | `"query_all"`      | `Some(rows.len())`   |

On failure the same operation string goes to `record_query_error`, and the `DbErr` is propagated unchanged.

### Transactions

`TransactionTrait::begin` on `InstrumentedConnection` times the `BEGIN` and reports `record_transaction_begin` on success. A *failed* begin is reported through `record_query_error("begin", ..)`, not a dedicated hook.

Begin returns a plain `DatabaseTransaction` — it is **not** auto-instrumented. To keep per-statement metrics inside the transaction, wrap it yourself:

```rust
use pgorm::{ConnectionTrait, TransactionTrait};
use pgorm::metric::InstrumentedTransaction;

let mut conn = instrumented.get().await?;
let metrics = conn.metrics().clone();

let tx = conn.begin().await?;                       // record_transaction_begin
let tx = InstrumentedTransaction::new(tx, metrics);

tx.execute("INSERT INTO users (name) VALUES ($1)", &[&"John"]).await?;

tx.commit().await?;                                 // record_transaction_commit
```

Two behaviours to plan around:

- A failed `commit` is reported as `record_transaction_rollback` — PostgreSQL aborts the transaction when a commit fails — not through an error hook.
- `InstrumentedTransaction`'s `Drop` impl records nothing. Dropping an uncommitted transaction still rolls back and still emits the inner `DatabaseTransaction`'s `tracing::warn!("Transaction dropped without committing!")`, but no rollback metric is produced. If you need rollbacks counted, call `record_transaction_rollback` yourself on the error path.

`InstrumentedTransaction` does not implement `TransactionTrait`, and `inner()` yields only a shared reference, so nested transactions (savepoints) are not reachable through the instrumented wrapper.

### Custom Collectors

Implement the trait for your own type. All seven hooks are required, so a collector that only cares about queries still supplies empty bodies for the rest. The example below sketches a Prometheus-backed collector; substitute whichever backend you use — the hooks are ordinary async functions.

```rust
use async_trait::async_trait;
use pgorm::DbErr;
use pgorm::metric::{InstrumentedPool, MetricsCollector};
use prometheus::{Counter, Histogram};
use std::time::Duration;

#[derive(Clone)]
pub struct PrometheusMetrics {
    queries: Counter,
    query_duration: Histogram,
    connections: Counter,
}

impl PrometheusMetrics {
    pub fn new() -> Self {
        Self {
            queries: prometheus::register_counter!("db_queries_total", "Total database queries").unwrap(),
            query_duration: prometheus::register_histogram!("db_query_duration_seconds", "Query duration").unwrap(),
            connections: prometheus::register_counter!("db_connections_total", "Total connections").unwrap(),
        }
    }
}

#[async_trait]
impl MetricsCollector for PrometheusMetrics {
    async fn record_query_success(&self, _operation: &str, duration: Duration, _rows: Option<u64>) {
        self.queries.inc();
        self.query_duration.observe(duration.as_secs_f64());
    }

    async fn record_query_error(&self, _operation: &str, duration: Duration, _error: &DbErr) {
        self.queries.inc();
        self.query_duration.observe(duration.as_secs_f64());
    }

    async fn record_connection_acquired(&self, _duration: Duration) {
        self.connections.inc();
    }

    async fn record_connection_error(&self, _duration: Duration, _error: &DbErr) {}
    async fn record_transaction_begin(&self, _duration: Duration) {}
    async fn record_transaction_commit(&self, _duration: Duration) {}
    async fn record_transaction_rollback(&self, _duration: Duration) {}
}

let instrumented = InstrumentedPool::new(pool, PrometheusMetrics::new());
```

The `operation` argument is the natural label dimension: it partitions the five `ConnectionTrait` methods (plus `"begin"` on a failed begin) without any extra plumbing.

### Cost

- **Not wrapping is free.** `DatabasePool`, `DatabaseConnection`, and `DatabaseTransaction` contain no metrics code, so an application that never constructs a wrapper is unaffected.
- **Static dispatch.** The collector is a generic parameter, not a trait object — no vtable lookup, and swapping implementations is a type change.
- **What wrapping does cost**, on every operation and regardless of collector: two clock reads (`Instant::now()` plus `elapsed()`) and one boxed future per hook call, since `#[async_trait]` boxes each hook's future. `NoOpMetrics` elides the reporting work, not the timing or the box.

## Production Deployment Tips

### 1. Use pg_stat_statements in Production
```sql
-- Monitor top slow queries
SELECT 
    substring(query, 1, 50) AS short_query,
    calls,
    total_exec_time / calls AS avg_time_ms,
    total_exec_time,
    (100.0 * total_exec_time / sum(total_exec_time) OVER ()) AS percentage
FROM pg_stat_statements
ORDER BY total_exec_time DESC
LIMIT 20;

-- Reset statistics
SELECT pg_stat_statements_reset();
```

### 2. Configure Connection Pool Monitoring
```rust
use pgorm::DatabasePool;
use tokio::time::{interval, Duration};

async fn monitor_pool(pool: DatabasePool) {
    let mut interval = interval(Duration::from_secs(30));
    
    loop {
        interval.tick().await;
        let status = pool.status();
        
        if status.available == 0 {
            tracing::warn!("Connection pool exhausted!");
        }
        
        tracing::info!("Pool status: {}/{} connections available", 
            status.available, status.size);
    }
}
```

### 3. Alert on Connection Pool Health
```rust
use pgorm::DatabasePool;

async fn check_pool_health(pool: &DatabasePool) -> Result<(), &'static str> {
    let status = pool.status();
    
    if status.available == 0 {
        return Err("No available connections");
    }
    
    if status.available < status.size / 4 {
        tracing::warn!("Pool running low: {}/{}", status.available, status.size);
    }
    
    Ok(())
}
```

## Common Patterns

- **Development**: `LoggingMetrics` plus `log_statement = 'all'` for debugging
- **Staging**: `LoggingMetrics`, or a custom collector at coarse granularity
- **Production**: pg_stat_statements + a custom collector wired to your metrics backend
- **High-throughput**: PostgreSQL logging only, leaving pools unwrapped

Remember: the database itself is usually the best source of truth for query metrics. Application-level metrics should supplement, not replace, PostgreSQL's built-in observability.
