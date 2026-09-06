mod common;

use common::setup::{TestContext, count_rows, has_column, has_index, has_table};
use pgorm_migration::migrator::MigrationStatus;
use pgorm_migration::prelude::*;

// DATABASE_URL=postgres://postgres:postgres@127.0.0.1:54329 cargo test -p pgorm-migration

/// A fresh database has nothing applied; `up` installs the tracking table and
/// runs every pending migration in order.
// [spec:pgorm:def:migration.runner+1/test]
// [spec:pgorm:sem:migration.up+2/test]
// [spec:pgorm:sem:migration.name+3/test]    asserted names are file stems
// [spec:pgorm:req:migration.ledger-upgrade/test]    the fresh-install half: the legacy name is never created
// [spec:pgorm:def:macros.derive+1/test]    `DeriveMigrationName` names each migration after the file stem
#[tokio::test]
async fn fresh_install_applies_all_pending() -> Result<(), Error> {
    let ctx = TestContext::new("pgorm_migration_fresh").await;
    let db = &ctx.db;

    assert!(!has_table(db, "pgorm_migrations").await?);

    let pending =
        common::migrator::default::Migrator::get_pending_migrations(&db.get().await?).await?;
    assert_eq!(pending.len(), 5);
    assert_eq!(pending[0].name(), "m20220118_000001_create_cake_table");
    assert_eq!(pending[0].status(), MigrationStatus::Pending);

    common::migrator::default::Migrator::up(db.clone(), None).await?;

    assert!(has_table(db, "pgorm_migrations").await?);
    assert!(has_column(db, "pgorm_migrations", "checksum").await?);
    // A database pgorm built itself never carries the inherited name.
    assert!(!has_table(db, "seaql_migrations").await?);
    assert!(has_table(db, "cake").await?);
    assert!(has_table(db, "fruit").await?);
    assert!(has_index(db, "cake", "cake_name_index").await?);
    assert!(!has_index(db, "cake", "non_existent_index").await?);

    // One row from the ActiveModel seed, one from the query-builder seed.
    assert_eq!(count_rows(db, "cake").await?, 2);
    assert_eq!(count_rows(db, "pgorm_migrations").await?, 5);

    let applied =
        common::migrator::default::Migrator::get_applied_migrations(&db.get().await?).await?;
    assert_eq!(applied.len(), 5);
    assert_eq!(applied[0].name(), "m20220118_000001_create_cake_table");
    assert_eq!(applied[0].status(), MigrationStatus::Applied);
    assert!(
        common::migrator::default::Migrator::get_pending_migrations(&db.get().await?)
            .await?
            .is_empty()
    );

    ctx.delete().await;
    Ok(())
}

/// Re-running `up` on an already-migrated database is a no-op: no migration is
/// re-applied and no extra ledger row is written.
// [spec:pgorm:sem:migration.up+2/test]
#[tokio::test]
async fn repeated_up_is_idempotent() -> Result<(), Error> {
    let ctx = TestContext::new("pgorm_migration_idempotent").await;
    let db = &ctx.db;

    common::migrator::default::Migrator::up(db.clone(), None).await?;
    let first = count_rows(db, "cake").await?;

    common::migrator::default::Migrator::up(db.clone(), None).await?;
    common::migrator::default::Migrator::up(db.clone(), None).await?;

    assert_eq!(count_rows(db, "cake").await?, first);
    assert_eq!(count_rows(db, "pgorm_migrations").await?, 5);
    assert!(
        common::migrator::default::Migrator::get_pending_migrations(&db.get().await?)
            .await?
            .is_empty()
    );

    ctx.delete().await;
    Ok(())
}

/// `steps` bounds how many pending migrations are applied, and `status` reports
/// the split without altering it.
// [spec:pgorm:def:migration.runner+1/test]
// [spec:pgorm:sem:migration.up+2/test]
#[tokio::test]
async fn stepped_up_reports_status() -> Result<(), Error> {
    let ctx = TestContext::new("pgorm_migration_status").await;
    let db = &ctx.db;

    // `install` alone creates the ledger without applying anything.
    common::migrator::default::Migrator::install(&db.get().await?).await?;
    assert!(has_table(db, "pgorm_migrations").await?);
    assert!(!has_table(db, "cake").await?);

    common::migrator::default::Migrator::up(db.clone(), Some(0)).await?;
    assert!(!has_table(db, "cake").await?);

    common::migrator::default::Migrator::up(db.clone(), Some(1)).await?;
    assert!(has_table(db, "cake").await?);
    assert!(!has_table(db, "fruit").await?);

    let conn = db.get().await?;
    common::migrator::default::Migrator::status(&conn).await?;

    let applied = common::migrator::default::Migrator::get_applied_migrations(&conn).await?;
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].name(), "m20220118_000001_create_cake_table");

    let pending = common::migrator::default::Migrator::get_pending_migrations(&conn).await?;
    assert_eq!(pending.len(), 4);
    assert_eq!(pending[0].name(), "m20220118_000002_create_fruit_table");

    drop(conn);
    ctx.delete().await;
    Ok(())
}

/// `migration_table_name` is honoured everywhere, including by `install`.
// [spec:pgorm:def:migration.runner+1/test]
#[tokio::test]
async fn migration_table_name_is_overridable() -> Result<(), Error> {
    let ctx = TestContext::new("pgorm_migration_table_name").await;
    let db = &ctx.db;

    common::migrator::override_migration_table_name::Migrator::up(db.clone(), None).await?;

    assert!(has_table(db, "override_migration_table_name").await?);
    assert!(!has_table(db, "pgorm_migrations").await?);
    assert_eq!(count_rows(db, "override_migration_table_name").await?, 5);
    assert!(has_table(db, "cake").await?);

    ctx.delete().await;
    Ok(())
}

/// The whole `up` run shares one transaction, so a failing migration rolls back
/// every migration in the run along with the ledger rows.
// [spec:pgorm:sem:migration.up+2/test]
// [spec:pgorm:req:migration.up-only/test]    no rollback path other than the transaction
#[tokio::test]
async fn failed_migration_rolls_back_the_run() -> Result<(), Error> {
    let ctx = TestContext::new("pgorm_migration_abort").await;
    let db = &ctx.db;

    let err = common::migrator::abort::Migrator::up(db.clone(), None)
        .await
        .expect_err("the final migration must fail");
    assert!(matches!(err, Error::Custom(ref msg) if msg == "Abort migration"));

    assert!(!has_table(db, "cake").await?);
    assert!(!has_table(db, "fruit").await?);
    assert!(!has_table(db, "pgorm_migrations").await?);

    ctx.delete().await;
    Ok(())
}

/// Two migrations answering `name()` with the same string share one ledger row,
/// so the list is refused — by `status` and by `up` alike — before anything is
/// installed, read or applied.
// [spec:pgorm:sem:migration.name+3/test]
#[tokio::test]
async fn duplicate_migration_names_are_rejected() -> Result<(), Error> {
    use common::migrator::pinned::{Duplicated, REPEATED};

    let ctx = TestContext::new("pgorm_migration_duplicate").await;
    let db = &ctx.db;

    let conn = db.get().await?;
    let err = Duplicated::status(&conn)
        .await
        .expect_err("a repeated migration name must be refused");
    let Error::Custom(msg) = &err else {
        panic!("expected Error::Custom, got {err}");
    };
    assert!(msg.contains(REPEATED), "{msg}");
    // The name that appears once is not an offender.
    assert!(!msg.contains("m20240101_000002_other"), "{msg}");
    drop(conn);

    let err = Duplicated::up(db.clone(), None)
        .await
        .expect_err("a repeated migration name must be refused");
    assert!(
        matches!(&err, Error::Custom(msg) if msg.contains(REPEATED)),
        "{err}"
    );

    // Refused ahead of the ledger, so nothing was installed and nothing ran.
    assert!(!has_table(db, "pgorm_migrations").await?);
    assert!(!has_table(db, REPEATED).await?);

    ctx.delete().await;
    Ok(())
}

/// The advisory key is a stable function of the ledger's name: it does not move
/// between builds, and two migrators with different ledgers do not contend.
// [spec:pgorm:sem:migration.up+2/test]
#[test]
fn lock_key_is_derived_from_the_ledger_name() {
    assert_eq!(
        common::migrator::default::Migrator::lock_key(),
        4_841_830_261_500_369_544
    );
    assert_eq!(
        common::migrator::override_migration_table_name::Migrator::lock_key(),
        -6_722_091_628_467_453_339
    );
}

/// Two runners racing one fresh database both succeed: the loser waits on the
/// advisory lock, then recomputes the pending set and finds nothing to do.
// [spec:pgorm:sem:migration.up+2/test]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_runners_queue_on_the_advisory_lock() -> Result<(), Error> {
    let ctx = TestContext::new("pgorm_migration_concurrent").await;
    let first = ctx.db.clone();
    let second = ctx.second_pool();

    let a = tokio::spawn(async move { common::migrator::default::Migrator::up(first, None).await });
    let b =
        tokio::spawn(async move { common::migrator::default::Migrator::up(second, None).await });

    a.await.expect("the first runner panicked")?;
    b.await.expect("the second runner panicked")?;

    // One run's worth of ledger rows, and one run's worth of seed rows.
    assert_eq!(count_rows(&ctx.db, "pgorm_migrations").await?, 5);
    assert_eq!(count_rows(&ctx.db, "cake").await?, 2);

    ctx.delete().await;
    Ok(())
}

/// A migration edited after it was applied reports a different checksum than
/// the ledger recorded, and every entry point says so.
// [spec:pgorm:req:migration.checksum/test]
#[tokio::test]
async fn editing_an_applied_migration_is_detected() -> Result<(), Error> {
    use common::migrator::pinned::{Checksummed, Edited, REPEATED};

    let ctx = TestContext::new("pgorm_migration_checksum_drift").await;
    let db = &ctx.db;

    Checksummed::up(db.clone(), None).await?;
    assert!(has_table(db, REPEATED).await?);

    let conn = db.get().await?;
    let err = Edited::status(&conn)
        .await
        .expect_err("an edited migration must be reported");
    let Error::Custom(msg) = &err else {
        panic!("expected Error::Custom, got {err}");
    };
    assert!(msg.contains(REPEATED), "{msg}");
    assert!(msg.contains("digest-one"), "{msg}");
    assert!(msg.contains("digest-two"), "{msg}");
    drop(conn);

    let err = Edited::up(db.clone(), None)
        .await
        .expect_err("an edited migration must be reported");
    assert!(
        matches!(&err, Error::Custom(msg) if msg.contains("digest-two")),
        "{err}"
    );

    // The unedited migration still passes.
    Checksummed::up(db.clone(), None).await?;

    ctx.delete().await;
    Ok(())
}

/// A checksum that either side is missing is unverifiable, not drift: a row
/// recorded without one, and a migration that no longer reports one, both pass.
// [spec:pgorm:req:migration.checksum/test]
#[tokio::test]
async fn an_unverifiable_checksum_is_not_drift() -> Result<(), Error> {
    use common::migrator::pinned::{Checksummed, Unchecked};

    let recorded_without = TestContext::new("pgorm_migration_checksum_null").await;
    Unchecked::up(recorded_without.db.clone(), None).await?;
    // Adopting a checksum afterwards cannot verify the grandfathered row.
    Checksummed::up(recorded_without.db.clone(), None).await?;
    let conn = recorded_without.db.get().await?;
    Checksummed::status(&conn).await?;
    assert_eq!(Checksummed::get_applied_migrations(&conn).await?.len(), 1);
    drop(conn);
    recorded_without.delete().await;

    let stopped_reporting = TestContext::new("pgorm_migration_checksum_dropped").await;
    Checksummed::up(stopped_reporting.db.clone(), None).await?;
    // Absence is not evidence of a change.
    Unchecked::up(stopped_reporting.db.clone(), None).await?;
    let conn = stopped_reporting.db.get().await?;
    Unchecked::status(&conn).await?;
    assert_eq!(Unchecked::get_applied_migrations(&conn).await?.len(), 1);
    drop(conn);
    stopped_reporting.delete().await;

    Ok(())
}

/// A database last migrated under the inherited `seaql_migrations` name is
/// adopted rather than re-run: the ledger answers to the new name, keeps every
/// row it had, and gains the checksum column on the way through.
// [spec:pgorm:req:migration.ledger-upgrade/test]    the upgrade half
// [spec:pgorm:req:migration.checksum/test]    an adopted ledger is widened by the same install
#[tokio::test]
async fn a_legacy_ledger_is_adopted_not_rerun() -> Result<(), Error> {
    let ctx = TestContext::new("pgorm_migration_legacy_adopt").await;
    let db = &ctx.db;

    // Manufacture a deployment predating both the rename and the checksum
    // column: migrate part-way, then put the ledger back the way an older
    // pgorm — or SeaORM — would have left it.
    common::migrator::default::Migrator::up(db.clone(), Some(2)).await?;
    let conn = db.get().await?;
    conn.execute("ALTER TABLE \"pgorm_migrations\" DROP COLUMN checksum", &[])
        .await?;
    conn.execute(
        "ALTER TABLE \"pgorm_migrations\" RENAME TO \"seaql_migrations\"",
        &[],
    )
    .await?;
    assert!(!has_table(db, "pgorm_migrations").await?);
    assert_eq!(count_rows(db, "seaql_migrations").await?, 2);

    // Reading the status is enough to adopt: without it these two would read
    // as pending and be applied a second time.
    let pending = common::migrator::default::Migrator::get_pending_migrations(&conn).await?;
    assert_eq!(pending.len(), 3);
    assert_eq!(pending[0].name(), "m20220118_000003_seed_cake_table");

    assert!(has_table(db, "pgorm_migrations").await?);
    assert!(!has_table(db, "seaql_migrations").await?);
    assert!(has_column(db, "pgorm_migrations", "checksum").await?);

    // Every row survived the move, and each is grandfathered as unverifiable.
    let applied = common::migrator::default::Migrator::get_applied_migrations(&conn).await?;
    assert_eq!(applied.len(), 2);
    assert_eq!(applied[0].name(), "m20220118_000001_create_cake_table");
    assert_eq!(applied[1].name(), "m20220118_000002_create_fruit_table");
    let unverifiable: i64 = conn
        .query_one(
            "SELECT COUNT(*) FROM \"pgorm_migrations\" WHERE checksum IS NULL",
            &[],
        )
        .await?
        .get(0);
    assert_eq!(unverifiable, 2);
    drop(conn);

    // The run picks up where the legacy ledger left off.
    common::migrator::default::Migrator::up(db.clone(), None).await?;
    assert_eq!(count_rows(db, "pgorm_migrations").await?, 5);
    assert_eq!(count_rows(db, "cake").await?, 2);

    ctx.delete().await;
    Ok(())
}

/// Adoption happens once. A later run has nothing to rename, and a legacy table
/// that reappears beside an existing ledger is left where it stands rather than
/// overwriting the ledger in use.
// [spec:pgorm:req:migration.ledger-upgrade/test]    idempotent, and never clobbering
#[tokio::test]
async fn legacy_adoption_is_idempotent() -> Result<(), Error> {
    let ctx = TestContext::new("pgorm_migration_legacy_idempotent").await;
    let db = &ctx.db;

    common::migrator::default::Migrator::up(db.clone(), None).await?;
    let conn = db.get().await?;
    conn.execute(
        "ALTER TABLE \"pgorm_migrations\" RENAME TO \"seaql_migrations\"",
        &[],
    )
    .await?;

    common::migrator::default::Migrator::install(&conn).await?;
    common::migrator::default::Migrator::install(&conn).await?;
    assert!(!has_table(db, "seaql_migrations").await?);
    assert_eq!(count_rows(db, "pgorm_migrations").await?, 5);

    // A stray table under the old name, with the ledger already in place: the
    // ledger in use wins and the stray is not read, renamed, or dropped.
    conn.execute(
        "CREATE TABLE \"seaql_migrations\" (\
         version TEXT NOT NULL PRIMARY KEY, applied_at BIGINT NOT NULL)",
        &[],
    )
    .await?;
    conn.execute(
        "INSERT INTO \"seaql_migrations\" (version, applied_at) VALUES ('stray', 0)",
        &[],
    )
    .await?;
    drop(conn);

    common::migrator::default::Migrator::up(db.clone(), None).await?;
    assert_eq!(count_rows(db, "pgorm_migrations").await?, 5);
    assert_eq!(count_rows(db, "seaql_migrations").await?, 1);

    ctx.delete().await;
    Ok(())
}

/// Adoption is reached from autocommit accessors as well as from `up`, so the
/// rename is guarded on the server too: racing installers all succeed and
/// exactly one of them renames the table.
// [spec:pgorm:req:migration.ledger-upgrade/test]    concurrent adoption
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_installers_adopt_a_legacy_ledger_once() -> Result<(), Error> {
    let ctx = TestContext::new("pgorm_migration_legacy_concurrent").await;

    common::migrator::default::Migrator::up(ctx.db.clone(), Some(2)).await?;
    let conn = ctx.db.get().await?;
    conn.execute("ALTER TABLE \"pgorm_migrations\" DROP COLUMN checksum", &[])
        .await?;
    conn.execute(
        "ALTER TABLE \"pgorm_migrations\" RENAME TO \"seaql_migrations\"",
        &[],
    )
    .await?;
    drop(conn);

    let racers: Vec<_> = (0..4)
        .map(|_| {
            let pool = ctx.second_pool();
            tokio::spawn(async move {
                let conn = pool.get().await?;
                common::migrator::default::Migrator::install(&conn).await
            })
        })
        .collect();
    for racer in racers {
        racer.await.expect("an installer panicked")?;
    }

    assert!(has_table(&ctx.db, "pgorm_migrations").await?);
    assert!(!has_table(&ctx.db, "seaql_migrations").await?);
    assert!(has_column(&ctx.db, "pgorm_migrations", "checksum").await?);
    assert_eq!(count_rows(&ctx.db, "pgorm_migrations").await?, 2);

    ctx.delete().await;
    Ok(())
}

/// A ledger the caller named is the caller's: a legacy table sitting beside it
/// is neither adopted from nor touched.
// [spec:pgorm:req:migration.ledger-upgrade/test]    an override opts out of adoption
#[tokio::test]
async fn a_custom_ledger_name_is_not_adopted() -> Result<(), Error> {
    let ctx = TestContext::new("pgorm_migration_legacy_custom").await;
    let db = &ctx.db;

    let conn = db.get().await?;
    conn.execute(
        "CREATE TABLE \"seaql_migrations\" (\
         version TEXT NOT NULL PRIMARY KEY, applied_at BIGINT NOT NULL)",
        &[],
    )
    .await?;
    conn.execute(
        "INSERT INTO \"seaql_migrations\" (version, applied_at) \
         VALUES ('m20220118_000001_create_cake_table', 0)",
        &[],
    )
    .await?;
    drop(conn);

    common::migrator::override_migration_table_name::Migrator::up(db.clone(), None).await?;

    // The custom ledger was built from nothing, so all five ran.
    assert_eq!(count_rows(db, "override_migration_table_name").await?, 5);
    assert!(!has_table(db, "pgorm_migrations").await?);
    // The legacy table is still exactly as it was found.
    assert_eq!(count_rows(db, "seaql_migrations").await?, 1);
    assert!(!has_column(db, "seaql_migrations", "checksum").await?);

    ctx.delete().await;
    Ok(())
}

/// A ledger deployed before the checksum column existed is widened in place,
/// keeping the rows it already had.
// [spec:pgorm:req:migration.checksum/test]
#[tokio::test]
async fn install_widens_a_pre_checksum_ledger() -> Result<(), Error> {
    let ctx = TestContext::new("pgorm_migration_checksum_widen").await;
    let db = &ctx.db;

    let conn = db.get().await?;
    conn.execute(
        "CREATE TABLE \"pgorm_migrations\" (\
         version TEXT NOT NULL PRIMARY KEY, applied_at BIGINT NOT NULL)",
        &[],
    )
    .await?;
    conn.execute(
        "INSERT INTO \"pgorm_migrations\" (version, applied_at) \
         VALUES ('m20220118_000001_create_cake_table', 0)",
        &[],
    )
    .await?;
    assert!(!has_column(db, "pgorm_migrations", "checksum").await?);

    common::migrator::default::Migrator::install(&conn).await?;
    assert!(has_column(db, "pgorm_migrations", "checksum").await?);

    let applied = common::migrator::default::Migrator::get_applied_migrations(&conn).await?;
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].name(), "m20220118_000001_create_cake_table");
    assert_eq!(
        common::migrator::default::Migrator::get_pending_migrations(&conn)
            .await?
            .len(),
        4
    );

    drop(conn);
    ctx.delete().await;
    Ok(())
}
