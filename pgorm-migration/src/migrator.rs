use futures::Future;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Display;
use std::pin::Pin;
use std::time::SystemTime;
use tracing::info;

use super::{MigrationTrait, ledger};
use pgorm::pgorm_query::{ColumnDef, Iden, IntoIden, Order, Query, SelectStatement, Table};
use pgorm::{
    ActiveModelTrait, ConnectionTrait, DatabasePool, DatabaseTransaction, DynIden, Error,
    FromQueryResult, Insert, Iterable, TransactionTrait, set,
};

/// The name of the ledger's nullable digest column, as PostgreSQL stores it.
const CHECKSUM_COLUMN: &str = "checksum";

/// The table `migration_table_name()` resolves to unless a migrator overrides it.
// [spec:pgorm:def:migration.runner+1]    the ledger's default physical name
pub const DEFAULT_LEDGER_TABLE: &str = "pgorm_migrations";

/// The name this crate inherited from SeaORM and no longer creates. A database
/// last migrated by SeaORM, or by pgorm before the rename, keeps its ledger
/// here; `install` adopts it rather than leaving it to read as unmigrated.
// [spec:pgorm:req:migration.ledger-upgrade]    the name that is looked for
pub const LEGACY_LEDGER_TABLE: &str = "seaql_migrations";

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// Status of migration
// [spec:pgorm:def:migration.runner+1]    reported status vocabulary
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
// [spec:pgorm:def:migration.runner+1]    runner surface
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
    // [spec:pgorm:req:migration.ledger-upgrade]    an override opts out of adoption
    fn migration_table_name() -> DynIden {
        ledger::Entity.into_iden()
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

        let name = Iden::to_string(&*Self::migration_table_name());
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

    /// Get list of applied migrations from database
    async fn get_migration_models(
        db: &(impl ConnectionTrait),
    ) -> Result<Vec<ledger::Model>, Error> {
        Self::install(db).await?;
        let stmt = Query::select()
            .table_name(Self::migration_table_name())
            .columns(ledger::Column::iter().map(IntoIden::into_iden))
            .order_by(ledger::Column::Version, Order::Asc)
            .to_owned();
        let (stmt, values) = stmt.build();
        ledger::Model::find_by_statement(stmt, values.0)
            .all(db)
            .await
    }

    /// Get list of migrations with status
    // [spec:pgorm:sem:migration.up+2]    pending set difference + missing-file detection
    // [spec:pgorm:req:migration.checksum]    recorded digests are checked on every read
    async fn get_migration_with_status(
        db: &(impl ConnectionTrait),
    ) -> Result<Vec<Migration>, Error> {
        let mut migration_files = Self::get_migration_files()?;
        Self::install(db).await?;
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

    /// Get list of pending migrations
    async fn get_pending_migrations(db: &(impl ConnectionTrait)) -> Result<Vec<Migration>, Error> {
        Ok(Self::get_migration_with_status(db)
            .await?
            .into_iter()
            .filter(|file| file.status == MigrationStatus::Pending)
            .collect())
    }

    /// Get list of applied migrations
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
    // [spec:pgorm:req:migration.ledger-upgrade]    detect, then rename in place
    async fn adopt_legacy_ledger(db: &(impl ConnectionTrait)) -> Result<(), Error> {
        if Iden::to_string(&*Self::migration_table_name()) != DEFAULT_LEDGER_TABLE {
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

    /// Create migration table `pgorm_migrations` in the database
    // [spec:pgorm:def:migration.runner+1]    self-provisioning ledger under migration_table_name()
    // [spec:pgorm:req:migration.checksum]    a ledger predating the column is widened in place
    // [spec:pgorm:req:migration.ledger-upgrade]    adoption precedes creation, widening follows it
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
        // nothing, and `install` runs on every read.
        let table_name = Iden::to_string(&*Self::migration_table_name());
        let has_checksum: bool = db
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
                 WHERE table_schema = current_schema() AND table_name = $1 AND column_name = $2)",
                &[&table_name, &CHECKSUM_COLUMN],
            )
            .await?
            .get(0);
        if !has_checksum {
            let stmt = Table::alter(Self::migration_table_name())
                .add_column_if_not_exists(ColumnDef::new(ledger::Column::Checksum).text().null());
            db.execute(&stmt.to_string(), &[]).await?;
            tracing::debug!("Widened the migration ledger with a checksum column");
        }

        tracing::debug!("Installed");
        Ok(())
    }

    /// Check the status of all migrations
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
    // [spec:pgorm:req:migration.ledger-upgrade]
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

    fn table_name(self, table_name: DynIden) -> Self::Statement;
}

impl QueryTable for SelectStatement {
    type Statement = SelectStatement;

    fn table_name(mut self, table_name: DynIden) -> SelectStatement {
        self.from(table_name);
        self
    }
}

impl<A> QueryTable for pgorm::Insert<A>
where
    A: ActiveModelTrait,
{
    type Statement = pgorm::Insert<A>;

    fn table_name(mut self, table_name: DynIden) -> pgorm::Insert<A> {
        pgorm::QueryTrait::query(&mut self).into_table(table_name);
        self
    }
}
