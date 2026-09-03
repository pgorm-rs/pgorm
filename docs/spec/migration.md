# Migrations

`pgorm-migration` runs ordered, forward-only schema changes against a PostgreSQL
database and records what it has applied in a ledger table. The crate is small
by design: `src/lib.rs` declares the two author-facing traits, `src/migrator.rs`
holds the whole runner, `src/seaql_migrations.rs` is the ledger entity, and
`src/util.rs` derives a migration's identity from its filename.

Unlike SeaORM, there is no `SchemaManager` indirection — a migration receives
the live `DatabaseTransaction` and drives it through `ConnectionTrait` with
`pgorm_query` builders.

## The runner and its ledger

> [spec:pgorm:def:migration.runner]
> A migration is a `MigrationTrait` implementor: `MigrationName + Send + Sync`
> plus a single `async fn up(&self, tx: &DatabaseTransaction<'_>)`. A migrator is
> a `MigratorTrait` implementor whose only required method is
> `migrations() -> Vec<Box<dyn MigrationTrait>>`, the ordered list of every
> migration the project owns.
>
> `MigratorTrait` provides, on top of that list: `migration_table_name() ->
> DynIden` (defaulting to the `seaql_migrations` entity's iden),
> `install(db)` to create the ledger, `up(db, steps)` to apply pending
> migrations, `status(db)` to log each migration's state, and the
> `get_migration_files` / `get_migration_models` / `get_migration_with_status` /
> `get_pending_migrations` / `get_applied_migrations` accessors. Every method
> except `up` takes `&impl ConnectionTrait`, so they can be called on a pooled
> connection or inside a caller's transaction; `up` takes an owned
> `DatabasePool` because it acquires and manages its own connection.
>
> The ledger is a two-column table — `version TEXT NOT NULL PRIMARY KEY` and
> `applied_at BIGINT NOT NULL` — mirrored by the `seaql_migrations::Model`
> entity. `install` creates it with `IF NOT EXISTS` under
> `migration_table_name()`, so an overridden name is honoured on creation as
> well as on read and write; every accessor calls `install` first, making the
> ledger self-provisioning. Results are reported as `Migration` values exposing
> `name()` and `status()`, where `MigrationStatus` is `Pending` or `Applied`
> and `Display`s as those words.

> [spec:pgorm:sem:migration.up+1]
> `up(db, steps)` acquires one pooled connection, opens one transaction, and
> runs the entire batch inside it before committing. Within that transaction it
> installs the ledger, computes the pending set, and applies migrations in
> `migrations()` declaration order — not sorted by name — passing each the
> shared `&DatabaseTransaction`. After each successful `up` it inserts a ledger
> row whose `version` is the migration's name and whose `applied_at` is the
> current Unix time in seconds.
>
> Pending detection is a set difference on names: every name in `migrations()`
> that has no ledger row is pending, and everything else is `Applied`. This is
> order-insensitive, so a migration inserted earlier in the list than
> already-applied ones is still detected and applied. `steps` bounds the batch
> and is checked before each migration, so `Some(0)` applies nothing and
> `None` applies everything pending. Re-running `up` with no new migrations is
> therefore a no-op.
>
> Because the whole batch shares one transaction, any error — from a
> migration's own `up` or from the ledger insert — aborts and rolls back every
> migration in that run together with its ledger rows and, on a fresh database,
> the ledger table itself. There is no partial-batch commit and no per-migration
> checkpoint.
>
> The reverse direction is also checked: if the ledger holds a version with no
> corresponding entry in `migrations()`, `get_migration_with_status` fails with
> `Error::Custom` naming each missing migration, so a deleted or renamed
> migration file surfaces as an error rather than silently re-running.

## Forward-only by construction

> [spec:pgorm:req:migration.up-only]
> Migrations MUST be forward-only. `MigrationTrait` declares `up` and nothing
> else, and `MigratorTrait` offers no `down`, `fresh`, `refresh`, or `reset`.
> The ledger is append-only in practice: nothing in the crate deletes a
> migration row. A mistake is corrected by writing a new migration, never by
> rolling one back.
>
> This is a deliberate divergence from SeaORM, whose `down` half was removed
> along with the `SchemaManager` wrapper that existed largely to make reversible
> DDL expressible. A `down` written months earlier is rarely exercised, cannot
> restore data the corresponding `up` dropped, and gives false confidence in
> production recovery; forward-only migrations keep the applied history a
> monotonic, auditable log. Callers needing a reversal write it as the next
> migration.

> [spec:pgorm:sem:migration.name+2]
> A migration's identity — the value written to and matched against the
> ledger's `version` column — is its **source filename** without extension, not
> its type name. `DeriveMigrationName` implements `MigrationName::name` as
> `util::get_file_stem(file!())`, which takes the `file!()` path's file stem, so
> every migration in a project must live in its own uniquely-named file and the
> conventional `m<timestamp>_<description>.rs` naming is what makes
> declaration order legible. `get_file_stem` is public API — the derive expands
> in the caller's crate — so it is reachable with any string and MUST be total:
> a path carrying no file stem, or one that is not valid UTF-8, yields that path
> unchanged rather than panicking.
>
> The consequence is that renaming a migration file renames the migration:
> the old name remains in the ledger with no matching entry in `migrations()`,
> which `get_migration_with_status` reports as a missing-migration
> `Error::Custom`, and the new name is treated as pending and re-applied.
> Implementors may bypass the derive and write `MigrationName` by hand to pin a
> name independently of the filename.
