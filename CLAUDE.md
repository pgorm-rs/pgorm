# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

pgorm is a fork of SeaORM focused entirely on PostgreSQL support. It uses tokio-postgres as the database driver and deadpool for connection pooling, with significant performance and stability improvements over the original SeaORM.

## Commands

### Testing
- `cargo nextest run --workspace` - Run the test suite (the preferred runner; skips doctests)
- `cargo test --doc --workspace` - Run the doctests, which nextest skips; the whole-workspace suite passes (a handful are deliberately ignored)
- Tests require a running PostgreSQL instance. Set `DATABASE_URL` to the *server* URL with no database path, e.g. `DATABASE_URL=postgres://postgres:postgres@localhost:5432`
- `.env.local` and `.env` are loaded automatically via dotenvy, so `DATABASE_URL` can live in either

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
- `pgorm-sql-macro` - The `sql!` and `prql!` macros, both always in scope (`pgorm::sql`, `pgorm::prql`; there is no gating feature). `sql!` holds a raw SQL string literal to the real PostgreSQL grammar (libpg_query) at compile time; `prql!` compiles PRQL text through prqlc at build time, validates the emitted SQL the same way, and arity-checks its `$N` placeholders against the macro arguments, expanding to `(&'static str, pgorm::Values)`

`pgorm::pipeline` (in-crate module, always compiled) is a PRQL-shaped composable query frontend: relation-to-relation transforms compiled through prqlc's PL AST to PostgreSQL SQL, with bound parameters minted by a per-pipeline binder (branded lifetimes make cross-pipeline placeholder mixing a compile error) and terminals landing on the ordinary decode paths. prqlc (pure Rust) is a plain, exact-pinned dependency.

There is no CLI crate: the inherited `pgorm-cli` was retired (it targeted sqlx/sea-schema and a migration surface this fork dropped). Entity generation is available as a library through `pgorm-codegen`; a starter migration-crate template lives at `pgorm-migration/template/migration/`.

### Core Components
- **Entity System**: `src/entity/` - Entity definitions, active models, relations
- **Query System**: `src/query/` - Query builders
- **Executor**: `src/executor/` - Query execution and result handling
- **Database**: `src/database/` - Connection management and pooling
- **Schema**: `src/schema/` - DDL statement generation from entity definitions (`Schema::create_table_from_entity`, `create_enum_from_entity`, `create_index_from_entity`, `create_comments_from_entity`). This is generation only, not introspection.
- **Metrics**: `src/metric.rs` - Opt-in instrumentation wrappers

Row streaming is reachable through the public crate: `ConnectionTrait::query_raw` returns a tokio-postgres `RowStream`, and `src/executor/select.rs` decodes it into models — `stream` on `Select`, `SelectGraph`, `Selector`, and `SelectorRaw`, plus `stream_partial_model`, each yielding a `PinBoxSendStream`. pgorm no longer carries its own `Statement`/`StatementBuilder`: `src/database/statement.rs` was deleted, leaving `src/database/` as just `connection.rs` and `db_connection.rs`, and SQL now travels as text (`&str` or `&String`, the sealed `SqlText` bound) alongside its parameters.

### Key Differences from SeaORM
- PostgreSQL-only (no multi-database support)
- Uses tokio-postgres directly (no sqlx)
- deadpool for connection pooling
- Parameters are passed alongside the statement so it is prepared properly (no string interpolation)
- Scoped transactions: `TransactionTrait::begin(&mut self)` returns a `DatabaseTransaction<'_>` that borrows the parent exclusively
- `DatabasePool` deliberately does **not** implement `ConnectionTrait` — you must `pool.get()` a connection first
- ActiveValue fields are written with the free `set(..)` (`name: set("Apple")`) or `.into()`; both convert into the column type, so no `.to_owned()`. `ActiveValue::Set` is the pattern-matching spelling
- `ColumnTrait` carries `eq_col`/`ne_col`/`gt_col`/`gte_col`/`lt_col`/`lte_col`/`eq_expr` for column-to-column and column-to-expression predicates; `eq` and friends stay value-only so their `save_as` enum cast is never dropped
- `ModelTrait::into_active()` converts a model to its entity's ActiveModel with no destination annotation
- `select([..])` on the SELECT builders clears the default projection and projects the given list in one call — the pipeline's verb, in the ORM. A single item needs no wrapper, a homogeneous list is an array or `Vec`, a mixed list (two entities' columns, an expression, an alias token) is a tuple; a computed iterator stays `select_only()` + `columns(..)`, which are unchanged
- Failsafe behavior for empty `insert_many` operations

### Connections
`pgorm::connect(config: tokio_postgres::Config) -> DatabasePool` is infallible in its signature: pool construction failure panics rather than returning an `Error`. `connect_with_builder` takes a closure over the `PoolBuilder` for sizing and timeouts, and returns `Result` — the closure is caller input, so an unbuildable pool is an `Error`, not a panic.

`connect_with(config, tls, manager: ManagerConfig, build)` is the general entry point the other three delegate to, and the only route to TLS (any `MakeTlsConnect<Socket>` connector — `tokio-postgres-rustls`, `tokio-postgres-openssl`), to a `RecyclingMethod` other than `Fast`, or to a non-default `StatementCacheSize`. `ManagerConfig`, `RecyclingMethod`, `StatementCacheSize`, `PoolBuilder`, `NoTls`, `Socket`, `MakeTlsConnect` and `TlsConnect` are all re-exported from `pgorm`, so calling it needs no direct dependency on `pgorm-pool` or `tokio-postgres`. `DatabasePool` still has no public constructor beside it.

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

The two query hooks take a `QueryContext<'_>` — `operation()`, `sql()`, and `fingerprint()` — instead of a bare operation name. `fingerprint()` is libpg_query's constants-normalized query identity and requires the off-by-default `metrics-fingerprint` feature; without it pgorm takes no `pg_query` dependency and the answer is always `None`.

Note that `begin()` on an `InstrumentedConnection` returns a plain `DatabaseTransaction` — wrap it in `InstrumentedTransaction::new` to keep per-statement metrics inside the transaction.

#### PostgreSQL Native Observability
Still the best source of truth for query statistics:
- `log_statement = 'all'` - Log all SQL statements
- `log_duration = on` - Log statement execution times
- `pg_stat_statements` extension - Track query statistics and performance
- `auto_explain` - Log slow query execution plans
- Connection pool status via `DatabasePool::status()`
