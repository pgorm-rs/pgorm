use futures::Future;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Display;
use std::pin::Pin;
use std::time::SystemTime;
use tracing::info;

use super::{MigrationTrait, ledger};
use pgorm::pgorm_query::{
    ColumnDef, Expr, IntoName, Order, Query, SelectStatement, SqlName, Table,
};
use pgorm::{
    ActiveModelTrait, ConnectionTrait, DatabasePool, DatabaseTransaction, Error, FromQueryResult,
    Insert, Iterable, Name, TransactionTrait, set,
};

/// The name of the ledger's nullable digest column, as PostgreSQL stores it.
const CHECKSUM_COLUMN: &str = "checksum";

/// The table `migration_table_name()` resolves to unless a migrator overrides it.
// [spec:pgorm:def:migration.runner+3]    the ledger's default physical name
pub const DEFAULT_LEDGER_TABLE: &str = "pgorm_migrations";

/// The name this crate inherited from SeaORM and no longer creates. A database
/// last migrated by SeaORM, or by pgorm before the rename, keeps its ledger
/// here; `install` adopts it rather than leaving it to read as unmigrated.
// [spec:pgorm:req:migration.ledger-upgrade+1]    the name that is looked for
pub const LEGACY_LEDGER_TABLE: &str = "seaql_migrations";

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// Status of migration
// [spec:pgorm:def:migration.runner+3]    reported status vocabulary
pub enum MigrationStatus {
    /// Not yet applied
    Pending,
    /// Applied
    Applied,
}

impl Display for MigrationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = match self {
            MigrationStatus::Pending => "Pending",
            MigrationStatus::Applied => "Applied",
        };
        write!(f, "{status}")
    }
}

pub struct Migration {
    migration: Box<dyn MigrationTrait>,
    status: MigrationStatus,
}

impl Migration {
    /// Get migration name from MigrationName trait implementation
    pub fn name(&self) -> &str {
        self.migration.name()
    }

    /// Get migration status
    pub fn status(&self) -> MigrationStatus {
        self.status
    }
}

/// Performing migrations on a database
// [spec:pgorm:def:migration.runner+3]    runner surface
// [spec:pgorm:req:migration.up-only]    no down/fresh/refresh/reset
#[async_trait::async_trait]
pub trait MigratorTrait: Send {
    /// Vector of migrations in time sequence
    fn migrations() -> Vec<Box<dyn MigrationTrait>>;

    /// Name of the migration table, it is `pgorm_migrations` by default
    ///
    /// Overriding this takes the ledger out of the crate's hands: legacy-name
    /// adoption applies to the default name alone, so a custom-named ledger is
    /// never renamed and never adopted from.
    // [spec:pgorm:req:migration.ledger-upgrade+1]    an override opts out of adoption
    fn migration_table_name() -> Name {
        ledger::Entity.into_name()
    }

    /// Get list of migrations wrapped in `Migration` struct, failing if two of
    /// them answer `name()` with the same string
    // [spec:pgorm:sem:migration.name+3]    duplicates are rejected here, ahead of every other step
    fn get_migration_files() -> Result<Vec<Migration>, Error> {
        let migrations = Self::migrations();

        let mut seen: HashSet<&str> = HashSet::new();
        let mut duplicates: BTreeSet<String> = BTreeSet::new();
        for migration in &migrations {
            let name = migration.name();
            if !seen.insert(name) {
                duplicates.insert(name.to_owned());
            }
        }

        if !duplicates.is_empty() {
            let names = duplicates
                .iter()
                .map(|name| format!("'{name}'"))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(Error::Custom(format!(
                "Duplicate migration name(s): {names}. A migration's name is its ledger identity, \
                 so it must be unique; note that `DeriveMigrationName` uses the file stem alone, \
                 which makes two same-named files in different directories collide"
            )));
        }

        Ok(migrations
            .into_iter()
            .map(|migration| Migration {
                migration,
                status: MigrationStatus::Pending,
            })
            .collect())
    }

    /// The key this migrator's advisory lock is taken on: a stable FNV-1a hash
    /// of the ledger table's name, so two runners contend exactly when they
    /// would write the same ledger, and never with an unrelated application
    /// lock that happened to pick a round number.
    ///
    /// The hash is spelled out here rather than taken from `DefaultHasher`
    /// because the key has to agree between processes that need not share a
    /// compiler version, let alone a `HashMap` seed.
    // [spec:pgorm:sem:migration.up+2]
    fn lock_key() -> i64 {
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

        let name = SqlName::to_string(&*Self::migration_table_name());
        let mut hash = FNV_OFFSET;
        for byte in name.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash as i64
    }

    /// Take the migration advisory lock, blocking until any concurrent runner
    /// against the same ledger has committed or rolled back.
    ///
    /// The argument is a transaction rather than any `ConnectionTrait` because
    /// `pg_advisory_xact_lock` is released at end of transaction: taken in
    /// autocommit it would be surrendered before the caller's next statement,
    /// which is the shape of the bug rather than the fix.
    // [spec:pgorm:sem:migration.up+2]
    async fn lock(db: &DatabaseTransaction<'_>) -> Result<(), Error> {
        let key = Self::lock_key();
        db.execute("SELECT pg_advisory_xact_lock($1)", &[&key])
            .await?;
        tracing::debug!("Acquired migration advisory lock {key}");
        Ok(())
    }

    /// Get list of applied migrations from database.
    ///
    /// Reads and nothing else: the ledger is located in the catalogue rather
    /// than provisioned, so a database that has never been migrated answers
    /// with an empty list instead of gaining a table. A ledger still sitting
    /// under the legacy name is read where it stands, and one predating the
    /// checksum column is read without it.
    // [spec:pgorm:req:migration.read-only]    a read locates the ledger, never creates it
    async fn get_migration_models(
        db: &(impl ConnectionTrait),
    ) -> Result<Vec<ledger::Model>, Error> {
        let Some(ledger) = ReadableLedger::locate(db, Self::migration_table_name()).await? else {
            return Ok(Vec::new());
        };

        let mut stmt = Query::select().table_name(ledger.name);
        stmt.order_by(ledger::Column::Version, Order::Asc);
        if ledger.has_checksum {
            stmt.columns(ledger::Column::iter().map(IntoName::into_name));
        } else {
            // The column is absent rather than empty, so it cannot be named;
            // a bound text NULL stands in, which is the same answer every row
            // of a widened ledger would have given and the same answer the
            // three-valued checksum check grandfathers.
            // [spec:pgorm:req:migration.checksum+1]
            stmt.column(ledger::Column::Version.into_name())
                .column(ledger::Column::AppliedAt.into_name())
                .expr_as(
                    Expr::val(Option::<String>::None),
                    ledger::Column::Checksum.into_name(),
                );
        }

        let (stmt, values) = stmt.build();
        ledger::Model::find_by_statement(stmt, values.0)
            .all(db)
            .await
    }

    /// Get list of migrations with status
    // [spec:pgorm:sem:migration.up+2]    pending set difference + missing-file detection
    // [spec:pgorm:req:migration.checksum+1]    recorded digests are checked on every read
    // [spec:pgorm:req:migration.read-only]    status is computed from a read alone
    async fn get_migration_with_status(
        db: &(impl ConnectionTrait),
    ) -> Result<Vec<Migration>, Error> {
        let mut migration_files = Self::get_migration_files()?;
        let migration_models = Self::get_migration_models(db).await?;

        let applied_checksums: HashMap<String, Option<String>> = migration_models
            .into_iter()
            .map(|model| (model.version, model.checksum))
            .collect();
        let migration_in_db: HashSet<String> = applied_checksums.keys().cloned().collect();
        let migration_in_fs: HashSet<String> = migration_files
            .iter()
            .map(|file| file.migration.name().to_string())
            .collect();

        let pending_migrations = &migration_in_fs - &migration_in_db;
        let mut checksum_errors: Vec<String> = Vec::new();
        for migration_file in migration_files.iter_mut() {
            if pending_migrations.contains(migration_file.migration.name()) {
                continue;
            }
            migration_file.status = MigrationStatus::Applied;

            let recorded = applied_checksums
                .get(migration_file.migration.name())
                .and_then(Option::as_deref);
            if let (Some(recorded), Some(current)) = (recorded, migration_file.migration.checksum())
                && recorded != current
            {
                checksum_errors.push(format!(
                    "Migration '{}' was applied with checksum '{recorded}' but now reports '{current}', so its contents have changed since it was applied; correct it with a new migration rather than by editing this one",
                    migration_file.migration.name()
                ));
            }
        }

        let missing_migrations_in_fs = &migration_in_db - &migration_in_fs;
        let mut errors: Vec<String> = missing_migrations_in_fs
            .iter()
            .map(|missing_migration| {
                format!("Migration file of version '{missing_migration}' is missing, this migration has been applied but its file is missing")
            }).collect();
        errors.extend(checksum_errors);

        if !errors.is_empty() {
            Err(Error::Custom(errors.join("\n")))
        } else {
            Ok(migration_files)
        }
    }

    /// Get list of pending migrations.
    ///
    /// A read, like every accessor here: against a database with no ledger at
    /// all, every migration is pending and nothing is installed to say so.
    // [spec:pgorm:req:migration.read-only]
    async fn get_pending_migrations(db: &(impl ConnectionTrait)) -> Result<Vec<Migration>, Error> {
        Ok(Self::get_migration_with_status(db)
            .await?
            .into_iter()
            .filter(|file| file.status == MigrationStatus::Pending)
            .collect())
    }

    /// Get list of applied migrations.
    ///
    /// A read, like every accessor here.
    // [spec:pgorm:req:migration.read-only]
    async fn get_applied_migrations(db: &(impl ConnectionTrait)) -> Result<Vec<Migration>, Error> {
        Ok(Self::get_migration_with_status(db)
            .await?
            .into_iter()
            .filter(|file| file.status == MigrationStatus::Applied)
            .collect())
    }

    /// Take over a ledger left under the legacy `seaql_migrations` name, so a
    /// database migrated before the rename is not mistaken for a fresh one.
    ///
    /// The take-over is a rename, not a copy: one ledger exists at any moment,
    /// so there is no window in which two of them can disagree. It applies only
    /// when `migration_table_name()` is the default — a custom name is the
    /// caller's own, and is left exactly where they put it — and only when the
    /// new name is absent, so a database that already has both keeps both and
    /// the legacy table is not touched.
    // [spec:pgorm:req:migration.ledger-upgrade+1]    detect, then rename in place
    async fn adopt_legacy_ledger(db: &(impl ConnectionTrait)) -> Result<(), Error> {
        if SqlName::to_string(&*Self::migration_table_name()) != DEFAULT_LEDGER_TABLE {
            return Ok(());
        }

        // The steady state — for a fresh database and an upgraded one alike —
        // is that there is nothing to adopt, and this read settles it without
        // taking a lock or naming a relation the server must lock to resolve.
        let adoptable: bool = db
            .query_one(
                "SELECT to_regclass($1) IS NULL AND to_regclass($2) IS NOT NULL",
                &[&DEFAULT_LEDGER_TABLE, &LEGACY_LEDGER_TABLE],
            )
            .await?
            .get(0);
        if !adoptable {
            return Ok(());
        }

        // The condition is then re-tested inside the server, under the
        // migrator's advisory lock, because `install` is also reached from
        // accessors running in autocommit, where nothing orders two racing
        // adoptions. A `DO` block is a single statement and therefore a single
        // implicit transaction, which is exactly the extent a transaction-scoped
        // lock needs to cover; a caller already holding that lock — every `up` —
        // takes it re-entrantly and is unaffected.
        //
        // The lock orders the adopters but does not by itself settle the guard.
        // `to_regclass` resolves a name without taking a relation lock, so it
        // never processes the invalidation messages the winner's rename sent and
        // can still answer from a catalog snapshot taken before it: the loser
        // reaches the `ALTER`, whose own lock acquisition re-resolves the name
        // and finds it gone. Those two outcomes — the source renamed away, or
        // the target already there — are the race resolving itself in our
        // favour, not failures, so they are caught and the block does nothing.
        // The handler's subtransaction keeps them off a caller's transaction,
        // which inside `up` is carrying the whole migration batch.
        let key = Self::lock_key();
        db.execute(
            &format!(
                "DO $adopt$ BEGIN \
                 PERFORM pg_advisory_xact_lock({key}); \
                 IF to_regclass('{DEFAULT_LEDGER_TABLE}') IS NULL \
                 AND to_regclass('{LEGACY_LEDGER_TABLE}') IS NOT NULL THEN \
                 ALTER TABLE \"{LEGACY_LEDGER_TABLE}\" RENAME TO \"{DEFAULT_LEDGER_TABLE}\"; \
                 END IF; \
                 EXCEPTION WHEN undefined_table OR duplicate_table THEN NULL; \
                 END $adopt$"
            ),
            &[],
        )
        .await?;
        tracing::debug!("Adopted the legacy '{LEGACY_LEDGER_TABLE}' ledger");

        Ok(())
    }

    /// Create migration table `pgorm_migrations` in the database.
    ///
    /// The one provisioning entry point, and the only method here that issues
    /// DDL: it adopts a legacy-named ledger, creates the table if it is absent,
    /// and widens one that predates the checksum column. `up` calls it; no read
    /// does, so asking a question never needs the privileges to answer it by
    /// changing the schema.
    // [spec:pgorm:def:migration.runner+3]    self-provisioning ledger under migration_table_name()
    // [spec:pgorm:req:migration.checksum+1]    a ledger predating the column is widened in place
    // [spec:pgorm:req:migration.ledger-upgrade+1]    adoption precedes creation, widening follows it
    // [spec:pgorm:req:migration.read-only]    the write path a read is split away from
    async fn install(db: &(impl ConnectionTrait)) -> Result<(), Error> {
        // Ahead of the create, or the create would answer a legacy database
        // with an empty ledger beside the populated one.
        Self::adopt_legacy_ledger(db).await?;

        let stmt = Table::create(Self::migration_table_name())
            .if_not_exists()
            .col(
                ColumnDef::new(ledger::Column::Version)
                    .text()
                    .not_null()
                    .primary_key(),
            )
            .col(
                ColumnDef::new(ledger::Column::AppliedAt)
                    .big_integer()
                    .not_null(),
            )
            .col(ColumnDef::new(ledger::Column::Checksum).text().null())
            .to_owned();
        db.execute(&stmt.to_string(), &[]).await?;

        // A ledger created before the column existed is widened rather than
        // recreated, leaving its rows NULL. This runs after adoption so an
        // adopted table is widened by the same step, rather than needing one of
        // its own. The catalog is consulted first because `ADD COLUMN IF NOT
        // EXISTS` takes an ACCESS EXCLUSIVE lock even when it goes on to do
        // nothing, and this is the provisioning path a caller may reach often.
        let table_name = SqlName::to_string(&*Self::migration_table_name());
        if !has_checksum_column(db, &table_name).await? {
            let stmt = Table::alter(Self::migration_table_name())
                .add_column_if_not_exists(ColumnDef::new(ledger::Column::Checksum).text().null());
            db.execute(&stmt.to_string(), &[]).await?;
            tracing::debug!("Widened the migration ledger with a checksum column");
        }

        tracing::debug!("Installed");
        Ok(())
    }

    /// Check the status of all migrations.
    ///
    /// A read: it logs what it found and leaves the database exactly as it was,
    /// so a readiness probe or a CI gate can call it against a connection with
    /// no DDL privileges at all.
    // [spec:pgorm:req:migration.read-only]
    async fn status(db: &(impl ConnectionTrait)) -> Result<(), Error> {
        info!("Checking migration status");

        for Migration { migration, status } in Self::get_migration_with_status(db).await? {
            info!("Migration '{}'... {}", migration.name(), status);
        }

        Ok(())
    }

    /// Apply pending migrations
    // [spec:pgorm:sem:migration.up+2]
    async fn up(db: DatabasePool, steps: Option<u32>) -> Result<(), Error> {
        tracing::debug!("Applying migrations");
        exec_with_connection::<'_, _>(db, move |manager| {
            tracing::debug!("Exec up");
            Box::pin(async move { exec_up::<Self>(manager, steps).await })
        })
        .await
    }
}

/// Whether `table` already carries the ledger's nullable digest column.
///
/// Both halves of the crate need the answer and neither may guess it: `install`
/// widens a table that predates the column, and a read has to know whether the
/// column can be named at all.
// [spec:pgorm:req:migration.checksum+1]    the catalogue is what says the column exists
async fn has_checksum_column(db: &impl ConnectionTrait, table: &str) -> Result<bool, Error> {
    Ok(db
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
             WHERE table_schema = current_schema() AND table_name = $1 AND column_name = $2)",
            &[&table, &CHECKSUM_COLUMN],
        )
        .await?
        .get(0))
}

/// A ledger a read may use, as the catalogue reports it — never as a read would
/// like it to be.
///
/// This is the read path's counterpart to `install`: it answers what is there,
/// where `install` makes what should be there. The two facts a read needs are
/// which table holds the rows and whether that table has the checksum column,
/// because a ledger older than the column cannot have it projected by name.
// [spec:pgorm:req:migration.read-only]    reads resolve the ledger, they do not provision it
struct ReadableLedger {
    /// The table to select from: `migration_table_name()`, or the legacy name
    /// when a ledger is still sitting there unadopted.
    name: Name,
    /// Whether `name` has the `checksum` column yet.
    has_checksum: bool,
}

impl ReadableLedger {
    /// Locate the ledger for `configured` using catalogue reads alone.
    ///
    /// `None` means no ledger exists under either name, which a caller reads as
    /// nothing applied — the honest answer for a database that has never been
    /// migrated, and one that costs it no table.
    ///
    /// A ledger still under `LEGACY_LEDGER_TABLE` is read where it stands
    /// rather than reported as absent. Reporting it absent would recreate, in
    /// the read path, exactly the failure `migration.ledger-upgrade` exists to
    /// prevent: an upgraded deployment's entire applied history read as
    /// pending. Adoption stays `install`'s job, so the answer is the same
    /// before and after the rename and only `up` moves the table.
    ///
    /// The legacy name is consulted under adoption's own first gate — the
    /// configured name is the default — so a caller who named their own ledger
    /// is never silently answered from someone else's.
    // [spec:pgorm:req:migration.read-only]
    // [spec:pgorm:req:migration.ledger-upgrade+1]    an unadopted ledger is read, not renamed
    async fn locate(
        db: &impl ConnectionTrait,
        configured: Name,
    ) -> Result<Option<ReadableLedger>, Error> {
        let configured_text = SqlName::to_string(&*configured);
        let legacy = (configured_text == DEFAULT_LEDGER_TABLE).then_some(LEGACY_LEDGER_TABLE);

        // One catalogue read settles both names, and `to_regclass` takes no
        // relation lock to answer. `quote_ident` is what makes the lookup agree
        // with how the name is rendered everywhere else in the crate: a
        // mixed-case ledger is `"MyLedger"`, not the `myledger` a bare
        // identifier would fold to. A `NULL` second name — a caller's own
        // ledger, which is never adopted from — resolves to `NULL` and so
        // never matches.
        let found: Option<String> = db
            .query_one(
                "SELECT CASE WHEN to_regclass(quote_ident($1)) IS NOT NULL THEN $1 \
                 WHEN to_regclass(quote_ident($2)) IS NOT NULL THEN $2 END",
                &[&configured_text, &legacy],
            )
            .await?
            .get(0);

        let Some(found) = found else {
            return Ok(None);
        };
        let has_checksum = has_checksum_column(db, &found).await?;
        let name = if found == configured_text {
            configured
        } else {
            Name::runtime(found)
        };

        Ok(Some(ReadableLedger { name, has_checksum }))
    }
}

// [spec:pgorm:sem:migration.up+2]    one connection, one transaction for the whole batch
async fn exec_with_connection<'c, F>(db: DatabasePool, f: F) -> Result<(), Error>
where
    F: for<'b> Fn(
        &'b DatabaseTransaction<'_>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'b>>,
{
    let mut conn = db.get().await?;
    let transaction = conn.begin().await?;
    f(&transaction).await?;
    transaction.commit().await
}

// [spec:pgorm:sem:migration.up+2]    lock, then declaration-order application, step bound, ledger append
async fn exec_up<M>(db: &DatabaseTransaction<'_>, mut steps: Option<u32>) -> Result<(), Error>
where
    M: MigratorTrait + ?Sized,
{
    // A list that names the same migration twice cannot be applied coherently,
    // so it is rejected before the lock is taken and before the ledger exists —
    // there is no reason to make a concurrent runner queue behind it.
    // [spec:pgorm:sem:migration.name+3]
    M::get_migration_files()?;

    // Before anything reads or writes the ledger: a concurrent runner is made
    // to wait here rather than collide with the ledger's primary key or race
    // this transaction's `CREATE TABLE IF NOT EXISTS`. Whoever waits recomputes
    // the pending set below, after the winner's commit is visible, and finds
    // nothing left to do. Taking the lock first also puts the legacy-name
    // adoption inside `install` under it, so a batch cannot begin against a
    // ledger another runner is in the middle of renaming.
    // [spec:pgorm:req:migration.ledger-upgrade+1]
    M::lock(db).await?;
    M::install(db).await?;

    if let Some(steps) = steps {
        info!("Applying {} pending migrations", steps);
    } else {
        info!("Applying all pending migrations");
    }

    let migrations = M::get_pending_migrations(db).await?.into_iter();
    if migrations.len() == 0 {
        info!("No pending migrations");
    }

    for Migration { migration, .. } in migrations {
        if let Some(steps) = steps.as_mut() {
            if steps == &0 {
                break;
            }
            *steps -= 1;
        }
        info!("Applying migration '{}'", migration.name());
        migration.up(db).await?;
        info!("Migration '{}' has been applied", migration.name());
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|err| {
                Error::Custom(format!("system clock is before the Unix epoch: {err}"))
            })?;
        Insert::one(ledger::ActiveModel {
            version: set(migration.name()),
            applied_at: set(now.as_secs() as i64),
            checksum: set(migration.checksum()),
        })
        .table_name(M::migration_table_name())
        .exec(db)
        .await?;
    }

    Ok(())
}

trait QueryTable {
    type Statement;

    fn table_name(self, table_name: Name) -> Self::Statement;
}

impl QueryTable for SelectStatement {
    type Statement = SelectStatement;

    fn table_name(mut self, table_name: Name) -> SelectStatement {
        self.from(table_name);
        self
    }
}

impl<A> QueryTable for pgorm::Insert<A>
where
    A: ActiveModelTrait,
{
    type Statement = pgorm::Insert<A>;

    fn table_name(mut self, table_name: Name) -> pgorm::Insert<A> {
        pgorm::QueryTrait::query(&mut self).into_table(table_name);
        self
    }
}
