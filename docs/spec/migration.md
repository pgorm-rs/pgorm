# Migrations

`pgorm-migration` runs ordered, forward-only schema changes against a PostgreSQL
database and records what it has applied in a ledger table. The crate is small
by design: `src/lib.rs` declares the two author-facing traits, `src/migrator.rs`
holds the whole runner, `src/ledger.rs` is the ledger entity, and `src/util.rs`
derives a migration's identity from its filename.

Unlike SeaORM, there is no `SchemaManager` indirection — a migration receives
the live `DatabaseTransaction` and drives it through `ConnectionTrait` with
`pgorm_query` builders.

## The runner and its ledger

> [spec:pgorm:def:migration.runner+1]
> A migration is a `MigrationTrait` implementor: `MigrationName + Send + Sync`
> plus a single `async fn up(&self, tx: &DatabaseTransaction<'_>)`, and an
> optional `fn checksum(&self) -> Option<String>` defaulting to `None`. A
> migrator is a `MigratorTrait` implementor whose only required method is
> `migrations() -> Vec<Box<dyn MigrationTrait>>`, the ordered list of every
> migration the project owns.
>
> `MigratorTrait` provides, on top of that list: `migration_table_name() ->
> DynIden` (defaulting to the `ledger` entity's iden), `install(db)` to create
> the ledger, `adopt_legacy_ledger(db)` to take over one left under the
> inherited name, `up(db, steps)` to apply pending migrations, `status(db)` to
> log each migration's state, `lock_key()` and `lock(tx)` for the run's advisory
> lock, and the `get_migration_files` / `get_migration_models` /
> `get_migration_with_status` / `get_pending_migrations` /
> `get_applied_migrations` accessors. Every method except `up`, `lock_key` and
> `lock` takes `&impl ConnectionTrait`, so they can be called on a pooled
> connection or inside a caller's transaction; `up` takes an owned
> `DatabasePool` because it acquires and manages its own connection, and `lock`
> takes a `&DatabaseTransaction<'_>` because the lock it takes is
> transaction-scoped. `get_migration_files` is the one accessor that touches no
> database yet is fallible: it is where the name-uniqueness check lives.
>
> The ledger is a three-column table — `version TEXT NOT NULL PRIMARY KEY`,
> `applied_at BIGINT NOT NULL`, and a nullable `checksum TEXT` — mirrored by the
> `ledger::Model` entity, whose `checksum` field is `Option<String>`. `install`
> creates it with `IF NOT EXISTS` under `migration_table_name()`, so an
> overridden name is honoured on creation as well as on read and write; every
> accessor reaches `install` before reading, making the ledger
> self-provisioning. Results are reported as `Migration` values exposing
> `name()` and `status()`, where `MigrationStatus` is `Pending` or `Applied` and
> `Display`s as those words.
>
> The ledger's default physical name is `pgorm_migrations`, exposed as
> `DEFAULT_LEDGER_TABLE`, and the Rust module holding its entity is `ledger`.
> Neither carries the `seaql_migrations` name inherited from SeaORM: the module
> named a vendor rather than a role, and the table put another project's name in
> the schema of every database pgorm touches. `ledger` is the word the rest of
> this document already uses for the thing, and `pgorm_migrations` reads in
> `\dt` the way the old name did, so an operator recognises it without being
> told. The old table name survives in exactly one place — as
> `LEGACY_LEDGER_TABLE`, the name adoption looks for.

> [spec:pgorm:sem:migration.up+2]
> `up(db, steps)` acquires one pooled connection, opens one transaction, and
> runs the entire batch inside it before committing. The transaction's first
> statement is `SELECT pg_advisory_xact_lock($key)`, where `$key` is
> `lock_key()`: a stable FNV-1a hash of `migration_table_name()`, spelled out in
> the crate rather than delegated to `DefaultHasher` so that two processes built
> by different compilers still agree on it. Only then does it install the
> ledger — which is also where a legacy-named ledger is adopted, so the rename
> happens under the lock and no batch can begin against a ledger another runner
> is midway through moving — compute the pending set, and apply migrations in
> `migrations()` declaration order — not sorted by name — passing each the
> shared `&DatabaseTransaction`. After each successful `up` it inserts a ledger row
> whose `version` is the migration's name, whose `applied_at` is the current
> Unix time in seconds, and whose `checksum` is that migration's `checksum()`.
>
> The lock makes concurrent runners queue instead of collide. Without it the
> loser of a race fails on whatever it happened to hit first — a duplicate
> `version` key, or the catalog race inside two simultaneous `CREATE TABLE IF
> NOT EXISTS` on a fresh database — so parallel deploys of the same release
> fail intermittently on an error a retry would cure. Because the lock is
> transaction-scoped it is released by the winner's commit, and because the
> pending set is computed after the lock is held the waiter recomputes it
> against the winner's committed ledger and finds nothing to do. The lock
> orders runs; it is not what makes them safe — that is still the single
> transaction below.
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

> [spec:pgorm:sem:migration.name+3]
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
> Uniqueness is enforced, not assumed. `get_migration_files` collects the names
> and MUST fail with an `Error::Custom` naming every repeated one — sorted, so
> the message does not depend on hash order — before any other step: before the
> ledger is installed, before a status is computed, and before a migration is
> applied. That placement is the requirement. Duplicates are reachable through
> the derive, which sees only the file stem, so `a/m1.rs` and `b/m1.rs` collide;
> left unchecked they read as one ledger row, which makes `up` on a fresh
> database die on the `version` primary key with the whole batch rolled back,
> and makes a `steps`-bounded run worse still — both migrations report
> `Applied` while only one ever ran, so the ledger says a schema exists that
> does not.
>
> The consequence of the filename rule is that renaming a migration file renames
> the migration: the old name remains in the ledger with no matching entry in
> `migrations()`, which `get_migration_with_status` reports as a
> missing-migration `Error::Custom`, and the new name is treated as pending and
> re-applied. Implementors may bypass the derive and write `MigrationName` by
> hand to pin a name independently of the filename — which is also how two
> migrations that must share a filename are told apart.

## Detecting an edited migration

> [spec:pgorm:req:migration.checksum]
> The ledger matches on name alone, so editing a migration that has already run
> is invisible: the name still matches, the row still says `Applied`, and a
> database built from scratch afterwards diverges from the one built before
> while both ledgers read identically. `MigrationTrait::checksum(&self) ->
> Option<String>` is the opt-in against that. `exec_up` stores the value in the
> new row's `checksum` column, and `get_migration_with_status` — reached by
> `status`, by `up`, and by the pending/applied accessors — compares the stored
> value against what the migration reports now, failing with an `Error::Custom`
> that names the migration and both digests when they differ.
>
> The check is deliberately three-valued. A stored `NULL` is unverifiable and
> MUST pass: rows written before the column existed, and rows written by a
> migration that reports no checksum, are grandfathered rather than treated as
> drift. A migration that has stopped reporting a checksum likewise MUST pass,
> because absence is not evidence. Only a stored digest that disagrees with a
> reported one is an error.
>
> `checksum()` defaults to `None` and there is no derived answer. Nothing the
> crate can see is a faithful digest of "what this migration does": the name is
> already the ledger key, and hashing the source file would report a
> reformatted comment as a schema change while missing a changed constant in an
> included file. Opting in is an override the author writes, and the value is
> opaque to the runner — a hash of the DDL text and a hand-bumped version string
> are equally valid.
>
> Existing deployments carry a two-column ledger, and `CREATE TABLE IF NOT
> EXISTS` will not widen it, so `install` MUST also add the column to a table
> that predates it. It consults `information_schema.columns` first and issues
> the `ALTER TABLE ... ADD COLUMN IF NOT EXISTS` only when the column is
> missing, because `ADD COLUMN IF NOT EXISTS` takes an `ACCESS EXCLUSIVE` lock
> even in the case where it does nothing, and `install` runs ahead of every
> read — including, inside `up`, a read that would then hold that lock for the
> length of the migration batch.
>
> The widening step sits after legacy-name adoption in `install`, and is the
> only widening step there is. A ledger adopted from the old name is by
> definition older than the checksum column, so it arrives needing exactly what
> the widening already does; making adoption a separate provisioning path would
> mean two ways to reach a three-column ledger and two chances to disagree about
> its shape.

## Adopting a ledger left under the inherited name

> [spec:pgorm:req:migration.ledger-upgrade]
> Renaming the default ledger table cannot be done by changing the default. A
> deployment upgraded to the renamed crate would look at `pgorm_migrations`,
> find nothing, and read its entire applied history as pending — then re-run
> every migration against a schema that already has them, failing on the first
> `CREATE TABLE` if the operator is lucky and corrupting data if they are not.
> The rename is therefore paired with detection: `install` MUST adopt a ledger
> found under `LEGACY_LEDGER_TABLE` before it creates one under
> `migration_table_name()`.
>
> Adoption is `ALTER TABLE ... RENAME TO`, not a copy. Every row is preserved
> because none is moved, and at no instant do two ledgers exist that could
> disagree about what has been applied; a copy would have to decide what to do
> with the original, and every answer to that is worse. The primary key and
> indexes travel with the table under their original names, so an adopted ledger
> keeps a constraint called `seaql_migrations_pkey`. That is left alone
> deliberately: it is harmless, and it is the only remaining evidence of where
> the table came from.
>
> Three conditions gate it, all required. `migration_table_name()` MUST be the
> default — a custom name is the caller's own ledger, is not adopted from, and
> is never renamed, which also makes overriding it back to `seaql_migrations`
> the supported way to decline the rename entirely. The default name MUST be
> absent — a database holding both keeps both, the ledger in use wins, and the
> legacy table is left untouched rather than read or destroyed. The legacy name
> MUST be present, or there is nothing to adopt. All three are re-tested on
> every `install`, which makes adoption idempotent: it happens once, and every
> later call is a single catalog read that answers no.
>
> That catalog read is the fast path and MUST come first, because the guarded
> rename below serialises and the steady state — fresh databases and adopted
> ones alike — has nothing to serialise. When it does report an adoptable
> ledger, the rename is issued as one `DO` block that re-tests the condition
> server-side under `pg_advisory_xact_lock(lock_key())`. A `DO` block is a
> single statement and so a single implicit transaction, which is exactly the
> extent a transaction-scoped lock needs; `install` is reached from autocommit
> accessors as readily as from inside `up`'s transaction, and a caller already
> holding the lock takes it re-entrantly.
>
> The lock orders adopters but does not settle the guard, and the block MUST
> also catch `undefined_table` and `duplicate_table` from the rename itself.
> `to_regclass` resolves a name without taking a relation lock, so it never
> processes the invalidation messages a concurrent adopter's commit sent and can
> answer from a catalog snapshot older than that commit. The `ALTER` then
> re-resolves the name while acquiring its own lock and finds the source gone.
> Both that and its mirror — the target already present — are the race resolving
> in our favour, so they are caught and the block does nothing; the handler's
> subtransaction is what keeps them off the caller's transaction, which inside
> `up` is carrying the whole migration batch. Anything else propagates.
