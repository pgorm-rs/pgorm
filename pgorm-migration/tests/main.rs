mod common;

use common::setup::{TestContext, count_rows, has_index, has_table};
use pgorm_migration::migrator::MigrationStatus;
use pgorm_migration::prelude::*;

// DATABASE_URL=postgres://postgres:postgres@127.0.0.1:54329 cargo test -p pgorm-migration

/// A fresh database has nothing applied; `up` installs the tracking table and
/// runs every pending migration in order.
// [spec:pgorm:def:migration.runner/test]
// [spec:pgorm:sem:migration.up/test]
// [spec:pgorm:sem:migration.name/test]    asserted names are file stems
// [spec:pgorm:def:macros.derive/test]    `DeriveMigrationName` names each migration after the file stem
#[tokio::test]
async fn fresh_install_applies_all_pending() -> Result<(), DbErr> {
    let ctx = TestContext::new("pgorm_migration_fresh").await;
    let db = &ctx.db;

    assert!(!has_table(db, "seaql_migrations").await?);

    let pending =
        common::migrator::default::Migrator::get_pending_migrations(&db.get().await?).await?;
    assert_eq!(pending.len(), 5);
    assert_eq!(pending[0].name(), "m20220118_000001_create_cake_table");
    assert_eq!(pending[0].status(), MigrationStatus::Pending);

    common::migrator::default::Migrator::up(db.clone(), None).await?;

    assert!(has_table(db, "seaql_migrations").await?);
    assert!(has_table(db, "cake").await?);
    assert!(has_table(db, "fruit").await?);
    assert!(has_index(db, "cake", "cake_name_index").await?);
    assert!(!has_index(db, "cake", "non_existent_index").await?);

    // One row from the ActiveModel seed, one from the query-builder seed.
    assert_eq!(count_rows(db, "cake").await?, 2);
    assert_eq!(count_rows(db, "seaql_migrations").await?, 5);

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
// [spec:pgorm:sem:migration.up/test]
#[tokio::test]
async fn repeated_up_is_idempotent() -> Result<(), DbErr> {
    let ctx = TestContext::new("pgorm_migration_idempotent").await;
    let db = &ctx.db;

    common::migrator::default::Migrator::up(db.clone(), None).await?;
    let first = count_rows(db, "cake").await?;

    common::migrator::default::Migrator::up(db.clone(), None).await?;
    common::migrator::default::Migrator::up(db.clone(), None).await?;

    assert_eq!(count_rows(db, "cake").await?, first);
    assert_eq!(count_rows(db, "seaql_migrations").await?, 5);
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
// [spec:pgorm:def:migration.runner/test]
// [spec:pgorm:sem:migration.up/test]
#[tokio::test]
async fn stepped_up_reports_status() -> Result<(), DbErr> {
    let ctx = TestContext::new("pgorm_migration_status").await;
    let db = &ctx.db;

    // `install` alone creates the ledger without applying anything.
    common::migrator::default::Migrator::install(&db.get().await?).await?;
    assert!(has_table(db, "seaql_migrations").await?);
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
// [spec:pgorm:def:migration.runner/test]
#[tokio::test]
async fn migration_table_name_is_overridable() -> Result<(), DbErr> {
    let ctx = TestContext::new("pgorm_migration_table_name").await;
    let db = &ctx.db;

    common::migrator::override_migration_table_name::Migrator::up(db.clone(), None).await?;

    assert!(has_table(db, "override_migration_table_name").await?);
    assert!(!has_table(db, "seaql_migrations").await?);
    assert_eq!(count_rows(db, "override_migration_table_name").await?, 5);
    assert!(has_table(db, "cake").await?);

    ctx.delete().await;
    Ok(())
}

/// The whole `up` run shares one transaction, so a failing migration rolls back
/// every migration in the run along with the ledger rows.
// [spec:pgorm:sem:migration.up/test]
// [spec:pgorm:req:migration.up-only/test]    no rollback path other than the transaction
#[tokio::test]
async fn failed_migration_rolls_back_the_run() -> Result<(), DbErr> {
    let ctx = TestContext::new("pgorm_migration_abort").await;
    let db = &ctx.db;

    let err = common::migrator::abort::Migrator::up(db.clone(), None)
        .await
        .expect_err("the final migration must fail");
    assert!(matches!(err, DbErr::Custom(ref msg) if msg == "Abort migration"));

    assert!(!has_table(db, "cake").await?);
    assert!(!has_table(db, "fruit").await?);
    assert!(!has_table(db, "seaql_migrations").await?);

    ctx.delete().await;
    Ok(())
}
