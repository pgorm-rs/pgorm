# pgorm Observability and Metrics Guide

pgorm deliberately does **not** include built-in metrics to maintain zero performance overhead in the critical database execution path. Instead, this guide shows you how to add observability through PostgreSQL-native tools and custom instrumentation.

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
use pgorm::Database;

let pool = Database::connect(config);

// Get pool status
let status = pool.status();
println!("Pool connections - Available: {}, Used: {}", 
    status.available, status.size - status.available);

// Get pool tag (useful for multi-pool setups)
let tag = pool.tag();
println!("Pool: {}", tag);
```

## Custom Application Metrics

### Trait-Based Approach (Recommended)

pgorm provides a trait-based metrics system that allows zero-cost abstractions when metrics aren't needed, while providing full flexibility for custom implementations:

```rust
use pgorm::{Database, metric::{InstrumentedPool, MetricsCollector, LoggingMetrics, NoOpMetrics}};
use async_trait::async_trait;
use std::time::Duration;

// Create a pool with no-op metrics (zero cost)
let pool = Database::connect(config);
let instrumented_pool = InstrumentedPool::new(pool, NoOpMetrics);

// Or with logging metrics
let instrumented_pool = InstrumentedPool::new(pool, LoggingMetrics);

// Use it exactly like a regular pool
let conn = instrumented_pool.get().await?;
let rows = conn.query_all("SELECT * FROM users", &[]).await?;
```

### Custom Metrics Implementation

Implement your own metrics collector:

```rust
use pgorm::metric::MetricsCollector;
use async_trait::async_trait;
use std::time::Duration;
use prometheus::{Counter, Histogram};

#[derive(Clone)]
pub struct PrometheusMetrics {
    query_counter: Counter,
    query_duration: Histogram,
    connection_counter: Counter,
}

impl PrometheusMetrics {
    pub fn new() -> Self {
        Self {
            query_counter: prometheus::register_counter!("db_queries_total", "Total database queries").unwrap(),
            query_duration: prometheus::register_histogram!("db_query_duration_seconds", "Query duration").unwrap(),
            connection_counter: prometheus::register_counter!("db_connections_total", "Total connections").unwrap(),
        }
    }
}

#[async_trait]
impl MetricsCollector for PrometheusMetrics {
    async fn record_query_success(&self, operation: &str, duration: Duration, rows: Option<u64>) {
        self.query_counter.inc();
        self.query_duration.observe(duration.as_secs_f64());
    }
    
    async fn record_query_error(&self, operation: &str, duration: Duration, error: &pgorm::DbErr) {
        self.query_counter.inc();
        self.query_duration.observe(duration.as_secs_f64());
    }
    
    async fn record_connection_acquired(&self, duration: Duration) {
        self.connection_counter.inc();
    }
    
    // ... implement other methods
    async fn record_connection_error(&self, _duration: Duration, _error: &pgorm::DbErr) {}
    async fn record_transaction_begin(&self, _duration: Duration) {}
    async fn record_transaction_commit(&self, _duration: Duration) {}
    async fn record_transaction_rollback(&self, _duration: Duration) {}
}

// Use your custom metrics
let metrics = PrometheusMetrics::new();
let instrumented_pool = InstrumentedPool::new(pool, metrics);
```

### Transaction Instrumentation

Transactions are also instrumented automatically:

```rust
let mut conn = instrumented_pool.get().await?;
let tx = conn.begin().await?; // Recorded as transaction_begin

// All operations within the transaction are recorded
let result = tx.execute("INSERT INTO users (name) VALUES ($1)", &[&"John"]).await?;

tx.commit().await?; // Recorded as transaction_commit
```

### Basic Wrapper Pattern (Alternative)

For more complex scenarios, you can still create custom wrappers around pgorm types:

```rust
use pgorm::{DatabasePool, DatabaseConnection, ConnectionTrait, DbErr};
use std::time::{Duration, Instant};
use async_trait::async_trait;
use tokio_postgres::{ToStatement, types::ToSql};

pub struct MetricPool {
    inner: DatabasePool,
    metrics: MetricCollector,
}

pub struct MetricConnection {
    inner: DatabaseConnection,
    metrics: MetricCollector,
}

#[derive(Clone)]
pub struct MetricCollector {
    // Your metrics backend - Prometheus, OpenTelemetry, etc.
}

impl MetricPool {
    pub fn new(pool: DatabasePool, metrics: MetricCollector) -> Self {
        Self { inner: pool, metrics }
    }

    pub async fn get(&self) -> Result<MetricConnection, DbErr> {
        let start = Instant::now();
        let conn = self.inner.get().await;
        let elapsed = start.elapsed();

        match &conn {
            Ok(_) => self.metrics.record_connection_acquired(elapsed),
            Err(e) => self.metrics.record_connection_error(elapsed, e),
        }

        Ok(MetricConnection {
            inner: conn?,
            metrics: self.metrics.clone(),
        })
    }
}

#[async_trait]
impl ConnectionTrait for MetricConnection {
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        let start = Instant::now();
        let result = self.inner.execute(statement, params).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(rows) => self.metrics.record_query_success("execute", elapsed, *rows),
            Err(e) => self.metrics.record_query_error("execute", elapsed, e),
        }

        result
    }

    async fn query_one<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<tokio_postgres::Row, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        let start = Instant::now();
        let result = self.inner.query_one(statement, params).await;
        let elapsed = start.elapsed();

        match &result {
            Ok(_) => self.metrics.record_query_success("query_one", elapsed, 1),
            Err(e) => self.metrics.record_query_error("query_one", elapsed, e),
        }

        result
    }

    // ... implement other ConnectionTrait methods similarly
}

impl MetricCollector {
    fn record_connection_acquired(&self, duration: Duration) {
        // Log or send to your metrics system
        tracing::info!("Connection acquired in {:?}", duration);
    }

    fn record_connection_error(&self, duration: Duration, error: &DbErr) {
        tracing::error!("Connection failed after {:?}: {:?}", duration, error);
    }

    fn record_query_success(&self, operation: &str, duration: Duration, rows: u64) {
        tracing::debug!("{} succeeded in {:?}, {} rows", operation, duration, rows);
        // metrics::histogram!("db_query_duration", duration.as_millis() as f64)
        //     .tag("operation", operation)
        //     .tag("result", "success");
    }

    fn record_query_error(&self, operation: &str, duration: Duration, error: &DbErr) {
        tracing::warn!("{} failed after {:?}: {:?}", operation, duration, error);
        // metrics::histogram!("db_query_duration", duration.as_millis() as f64)
        //     .tag("operation", operation)
        //     .tag("result", "error");
    }
}
```

### Integration with Tracing

Using the `tracing` crate for structured logging:

```rust
use tracing::{info_span, Instrument};

#[async_trait]
impl ConnectionTrait for MetricConnection {
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        async move {
            let result = self.inner.execute(statement, params).await;
            
            match &result {
                Ok(rows) => tracing::info!(rows = %rows, "Query executed successfully"),
                Err(e) => tracing::error!(error = %e, "Query failed"),
            }
            
            result
        }
        .instrument(info_span!("db_execute"))
        .await
    }
}
```

### Prometheus Integration Example

```rust
use prometheus::{Counter, Histogram, register_counter, register_histogram};
use lazy_static::lazy_static;

lazy_static! {
    static ref QUERY_COUNTER: Counter = register_counter!(
        "pgorm_queries_total", 
        "Total number of database queries"
    ).unwrap();
    
    static ref QUERY_DURATION: Histogram = register_histogram!(
        "pgorm_query_duration_seconds",
        "Database query duration in seconds"
    ).unwrap();
    
    static ref CONNECTION_COUNTER: Counter = register_counter!(
        "pgorm_connections_total",
        "Total number of database connections acquired"
    ).unwrap();
}

impl MetricCollector {
    fn record_query_success(&self, operation: &str, duration: Duration, rows: u64) {
        QUERY_COUNTER.inc();
        QUERY_DURATION.observe(duration.as_secs_f64());
    }
    
    fn record_connection_acquired(&self, duration: Duration) {
        CONNECTION_COUNTER.inc();
    }
}
```

### OpenTelemetry Integration

```rust
use opentelemetry::{metrics::*, trace::*};

pub struct OtelMetricCollector {
    meter: Meter,
    query_counter: Counter<u64>,
    query_duration: Histogram<f64>,
}

impl OtelMetricCollector {
    pub fn new() -> Self {
        let meter = opentelemetry::global::meter("pgorm");
        
        let query_counter = meter
            .u64_counter("db_queries_total")
            .with_description("Total database queries")
            .init();
            
        let query_duration = meter
            .f64_histogram("db_query_duration")
            .with_description("Database query duration in seconds")
            .init();

        Self { meter, query_counter, query_duration }
    }

    fn record_query(&self, operation: &str, duration: Duration, success: bool) {
        let labels = &[
            KeyValue::new("operation", operation),
            KeyValue::new("success", success.to_string()),
        ];
        
        self.query_counter.add(1, labels);
        self.query_duration.record(duration.as_secs_f64(), labels);
    }
}
```

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

## Benefits of the Trait-Based Approach

1. **Zero Cost**: `NoOpMetrics` compiles to nothing - no runtime overhead
2. **Type Safe**: Metrics collector is part of the type system at compile time
3. **Flexible**: Implement exactly the metrics you need via the `MetricsCollector` trait
4. **Composable**: Can instrument any `ConnectionTrait` implementation
5. **Async Native**: All metrics collection methods are async for maximum flexibility
6. **Production Ready**: Static dispatch means no vtable overhead
7. **PostgreSQL Native**: Still leverages built-in database features for comprehensive monitoring

### Performance Characteristics

- **With `NoOpMetrics`**: Identical performance to unwrapped connections (zero cost)
- **With custom metrics**: Only the overhead of your chosen metrics backend
- **Static dispatch**: All method calls are resolved at compile time
- **No heap allocations**: Metrics collectors are typically stack-allocated or static

### Migration from Wrapper Pattern

The trait-based approach is superior to manual wrappers because:
- Less boilerplate code required
- Automatic instrumentation of all ConnectionTrait methods
- Compile-time guarantee that all operations are instrumented
- Easy to swap metrics implementations

## Common Patterns

- **Development**: Use `log_statement = 'all'` for debugging
- **Staging**: Add basic timing metrics with tracing
- **Production**: Use pg_stat_statements + minimal custom metrics
- **High-throughput**: PostgreSQL logging only, metrics via sampling

Remember: The database itself is usually the best source of truth for query metrics. Application-level metrics should supplement, not replace, PostgreSQL's built-in observability.