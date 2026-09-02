# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

pgorm is a fork of SeaORM focused entirely on PostgreSQL support. It uses tokio-postgres as the database driver and deadpool for connection pooling, with significant performance and stability improvements over the original SeaORM.

## Commands

### Testing
- `cargo nextest run --workspace` - Run the test suite (the preferred runner; skips doctests)
- `cargo test --doc --workspace` - Run the doctests, which nextest skips; the whole-workspace suite passes (a handful are deliberately ignored)
- Tests require a running PostgreSQL instance. Set `DATABASE_URL` to the *server* URL with no database path, e.g. `DATABASE_URL=postgres://postgres:postgres@localhost:5432`
- `.env.local` and `.env` are loaded automatically via dotenv, so `DATABASE_URL` can live in either

### Build and Development
- `cargo build` - Build the project
- `cargo check --workspace` - Check compilation without building
- `cargo clippy` - Run linter (the crate root denies `missing_debug_implementations`, `clippy::unwrap_used`, `clippy::missing_panics_doc`, and the print macros; `missing_docs` warns)
- `cargo fmt --all` - Format

## Architecture

### Workspace Structure
- `pgorm` (root) - Main ORM crate
- `pgorm-macros` - Derive macros for entities and models
- `pgorm-codegen` - Entity source generation from a described schema
- `pgorm-migration` - Migration runner
- `pgorm-pool` - Database connection pool (vendored deadpool-postgres fork)
- `pgorm-query` - SQL query builder (fork of sea-query, PostgreSQL-only), with its own `pgorm-query-attr` and `pgorm-query-derive` members

There is no CLI crate: the inherited `pgorm-cli` was retired (it targeted sqlx/sea-schema and a migration surface this fork dropped). Entity generation is available as a library through `pgorm-codegen`; a starter migration-crate template lives at `pgorm-migration/template/migration/`.

### Core Components
- **Entity System**: `src/entity/` - Entity definitions, active models, relations
- **Query System**: `src/query/` - Query builders
- **Executor**: `src/executor/` - Query execution and result handling
- **Database**: `src/database/` - Connection management and pooling
- **Schema**: `src/schema/` - DDL statement generation from entity definitions (`Schema::create_table_from_entity`, `create_enum_from_entity`, `create_index_from_entity`, `create_comments_from_entity`). This is generation only, not introspection.
- **Metrics**: `src/metric.rs` - Opt-in instrumentation wrappers

Row streaming is reachable through the public crate: `ConnectionTrait::query_raw` returns a tokio-postgres `RowStream`, and `src/executor/select.rs` decodes it into models — `stream` on `Select`, `SelectTwo`, `Selector`, and `SelectorRaw`, plus `stream_partial_model`, each yielding a `PinBoxSendStream`. pgorm no longer carries its own `Statement`/`StatementBuilder`: `src/database/statement.rs` was deleted, leaving `src/database/` as just `connection.rs` and `db_connection.rs`, and SQL now travels as a plain `&str` (anything `ToStatement`) alongside its parameters.

### Key Differences from SeaORM
- PostgreSQL-only (no multi-database support)
- Uses tokio-postgres directly (no sqlx)
- deadpool for connection pooling
- Parameters are passed alongside the statement so it is prepared properly (no string interpolation)
- Scoped transactions: `TransactionTrait::begin(&mut self)` returns a `DatabaseTransaction<'_>` that borrows the parent exclusively
- `DatabasePool` deliberately does **not** implement `ConnectionTrait` — you must `pool.get()` a connection first
- `From<T>` implementations for ActiveValue fields (less verbose than `ActiveValue::Set()`)
- Failsafe behavior for empty `insert_many` operations

### Connections
`pgorm::connect(config: tokio_postgres::Config) -> DatabasePool` is infallible in its signature: pool construction failure panics rather than returning a `DbErr`. `connect_with_builder` takes a closure over the `PoolBuilder` for sizing and timeouts, and returns `Result` — the closure is caller input, so an unbuildable pool is a `DbErr`, not a panic. TLS is not supported through these entry points (`NoTls` is hard-coded).

### Testing Setup
Tests use a common setup pattern in `tests/common/setup/mod.rs` that:
- Creates a throwaway database per test, named after the test, dropping any prior copy first (`DROP DATABASE ... WITH (FORCE)`)
- Connects to the `postgres` maintenance database to do so, then returns a pool for the new database
- Sets up schema and test data, and tears the database down afterwards via `TestContext::delete`
- Uses `pretty_assertions` for better test output

### Observability and Metrics

pgorm ships an opt-in metrics layer in `pgorm::metric` — see [METRICS.md](METRICS.md) for the full guide. The core types (`DatabasePool`, `DatabaseConnection`, `DatabaseTransaction`) carry no metrics hooks; instrumentation lives only in wrapper types the application chooses to construct, so unwrapped code pays nothing.

```rust
use pgorm::metric::{InstrumentedPool, LoggingMetrics};

let pool = InstrumentedPool::new(pgorm::connect(config), LoggingMetrics);
let conn = pool.get().await?; // records connection acquisition
```

`NoOpMetrics` and `LoggingMetrics` ship in-tree. Custom backends implement the `MetricsCollector` trait (async, `Clone + Send + Sync + 'static`, seven hooks, no defaults) rather than hand-rolling a wrapper.

Note that `begin()` on an `InstrumentedConnection` returns a plain `DatabaseTransaction` — wrap it in `InstrumentedTransaction::new` to keep per-statement metrics inside the transaction.

#### PostgreSQL Native Observability
Still the best source of truth for query statistics:
- `log_statement = 'all'` - Log all SQL statements
- `log_duration = on` - Log statement execution times
- `pg_stat_statements` extension - Track query statistics and performance
- `auto_explain` - Log slow query execution plans
- Connection pool status via `DatabasePool::status()`
