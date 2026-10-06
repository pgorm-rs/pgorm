# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

pgorm is a fork of SeaORM focused entirely on PostgreSQL support. It uses tokio-postgres as the database driver and deadpool for connection pooling, with significant performance and stability improvements over the original SeaORM.

## Commands

### Testing
- `cargo nextest run --workspace` - Run the test suite (the preferred runner; skips doctests)
- `cargo test --doc --workspace` - Run the doctests, which nextest skips entirely: a SQL golden written inside a doc example is checked here and nowhere else, so run this whenever rendering changes (~17 minutes). All nine crates pass — 543 examples, 9 of them `ignore`-fenced and never run. Two things to know when reading a result: cargo stops at the first crate that fails, so a failure count is a lower bound and later crates may simply not have run; and a doctest run shares `target/` with any other cargo invocation, so a concurrent `build`/`check`/`nextest` swaps the rlibs underneath it and manufactures failures that do not reproduce. Let it finish alone
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

Two dependencies are git forks pinned by exact rev, because a `[patch]` table reaches only the workspace declaring it: prqlc (`necessary-nu/prql`) and pg_query (`necessary-nu/pg_query.rs`, branch `pgorm/postgres-18`: upstream's unreleased 18.0.0 on libpg_query 18.1.0, so every parse in pgorm uses PostgreSQL 18.6's grammar). Each is declared at the same rev in every manifest that names it, allowed in `deny.toml`'s `allow-git`, and held there, with every committed lockfile including the detached workspaces under `security/` and `pgorm-python/`, by `tests/fork_pin_tests.rs`. Moving a fork's rev means editing every declaration and re-resolving every lockfile (`cargo metadata --manifest-path <dir>/Cargo.toml` re-locks one minimally). crates.io refuses git dependencies, so pgorm is not publishable there while they stand.

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
- `Insert` is typed by row count: `Insert::one` keeps `exec_returning_pk` / `exec_returning_model`, while `Insert::many` (and `add`/`add_many`) returns every row's key or model in insertion order through `exec_returning_pks` / `exec_returning_models`
- A zero-model insert batch writes nothing and sends no SQL: `Insert::many(empty)` reports `Ok(0)` through `exec` and an empty `Vec` through the returning terminals; an explicitly supplied all-`NotSet` model still writes one default row, and `TryInsert` reports `Empty` for both states
- Composite primary keys take their parts borrowed (`find_by_id((1, "x"))` against an `(i32, String)` key), and one of their columns can be generated by the database with `#[pgorm(identity)]` / `#[pgorm(identity_by_default)]`
- Every value predicate carries `save_as` casts (`between`, `if_null` and the membership forms included), and keyset cursor boundaries bind under the same casts; graph cursors complete the ordered source's continuation key with its own primary key
- `load_one`/`load_many` evaluate the complete authored relation (`on_condition` and `condition_type` included) by riding the graph machinery, and `load_one` errors on an unmatched key instead of yielding `None` silently
- `RelationDef::rev()` preserves an attached `on_condition`'s authored argument roles
- Enum types can be schema-qualified end to end: `ColumnType::Enum` carries `schema`, `DeriveActiveEnum` takes `schema_name = "..."`, and codegen keys enums by full identity with no cross-schema fallback
- Prefixed result-column names (the graph's `s{i}_` aliases and every prefixed decode) are bounded to PostgreSQL's 63-byte identifier limit through one shared composition

### Connections
`pgorm::connect(config: tokio_postgres::Config) -> DatabasePool` is infallible in its signature: pool construction failure panics rather than returning an `Error`. `connect_with_builder` takes a closure over the `PoolBuilder` for sizing and timeouts, and returns `Result` — the closure is caller input, so an unbuildable pool is an `Error`, not a panic.

`connect_with(config, tls, manager: ManagerConfig, build)` is the general entry point the other three delegate to, and the only route to TLS (any `MakeTlsConnect<Socket>` connector — `tokio-postgres-rustls`, `tokio-postgres-openssl`), to a `RecyclingMethod` other than `Fast`, or to a non-default `StatementCacheSize`. `ManagerConfig`, `RecyclingMethod`, `StatementCacheSize`, `PoolBuilder`, `NoTls`, `Socket`, `MakeTlsConnect` and `TlsConnect` are all re-exported from `pgorm`, so calling it needs no direct dependency on `pgorm-pool` or `tokio-postgres`. `DatabasePool` still has no public constructor beside it.

### Testing Setup
The tests target PostgreSQL 18, and nothing tests an older release. The local server is the docker container `pgorm-test`, created as:

```sh
docker volume create pgorm-test-18-data
docker run -d --name pgorm-test -p 54329:5432 -e POSTGRES_PASSWORD=postgres \
  -v pgorm-test-18-data:/var/lib/postgresql postgres:18
```

so `DATABASE_URL=postgres://postgres:postgres@127.0.0.1:54329`. The 18 image keeps its data under `/var/lib/postgresql/18/docker`, so a volume mounts at `/var/lib/postgresql`, not the `.../data` older images used, and a data directory from another major release cannot be reused. The same release runs everywhere else: CI's Python jobs install PostgreSQL 18 (the PostgreSQL apt repository on Ubuntu 24.04, `postgresql@18` on macOS), and pgorm-python's Docker wrapper (`pgorm-python/tests/with_postgres.py`) pins `postgres:18.6-bookworm` by digest.

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

The two query hooks take a `QueryContext<'_>` — `operation()`, `sql()`, and `fingerprint()` — instead of a bare operation name. `fingerprint()` is libpg_query's constants-normalized query identity, grouped by PostgreSQL 18's query-ID rules (an alias stands for its relation, schema qualifiers are ignored), and requires the off-by-default `metrics-fingerprint` feature; without it no statement is parsed for metrics and the answer is always `None` (pg_query is linked either way, for the paginator and the macros).

Note that `begin()` on an `InstrumentedConnection` returns a plain `DatabaseTransaction` — wrap it in `InstrumentedTransaction::new` to keep per-statement metrics inside the transaction.

#### PostgreSQL Native Observability
Still the best source of truth for query statistics:
- `log_statement = 'all'` - Log all SQL statements
- `log_duration = on` - Log statement execution times
- `pg_stat_statements` extension - Track query statistics and performance
- `auto_explain` - Log slow query execution plans
- Connection pool status via `DatabasePool::status()`
