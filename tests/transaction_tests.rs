#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, bakery_chain::*, setup::*};
use pgorm::{
    DatabaseConnection, DatabaseTransaction, IsolationLevel, TransactionError, TransactionMode,
    TransactionTrait, entity::prelude::*, set,
};
use pretty_assertions::assert_eq;
use tokio_postgres::error::SqlState;

async fn insert_bakery<C>(db: &C, name: &str, profit_margin: f64) -> Result<(), Error>
where
    C: ConnectionTrait,
{
    bakery::ActiveModel {
        name: set(name),
        profit_margin: set(profit_margin),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(())
}

async fn count_bakeries<C>(db: &C, search_name: &str) -> Result<usize, Error>
where
    C: ConnectionTrait,
{
    Ok(Bakery::find()
        .filter(bakery::Column::Name.contains(search_name))
        .all(db)
        .await?
        .len())
}

async fn show<C>(db: &C, setting: &str) -> Result<String, Error>
where
    C: ConnectionTrait,
{
    let row = db
        .query_one(format!("SHOW {setting}").as_str(), &[])
        .await?;
    Ok(row.get(0))
}

fn assert_read_only_violation(err: &Error) {
    match err {
        Error::Postgres(e) => assert_eq!(
            e.as_db_error().map(|e| e.code()),
            Some(&SqlState::READ_ONLY_SQL_TRANSACTION),
        ),
        other => panic!("expected Error::Postgres, got {other:?}"),
    }
}

#[pgorm_macros::test]
pub async fn transaction() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_test").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    let txn = db.begin().await?;

    insert_bakery(&txn, "SeaSide Bakery", 10.4).await?;
    insert_bakery(&txn, "Top Bakery", 15.0).await?;

    assert_eq!(count_bakeries(&txn, "Bakery").await?, 2);

    txn.commit().await?;

    assert_eq!(count_bakeries(&db, "Bakery").await?, 2);

    drop(db);
    ctx.delete().await;

    Ok(())
}

#[pgorm_macros::test]
pub async fn transaction_with_reference() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_with_reference_test").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    let name1 = "SeaSide Bakery";
    let name2 = "Top Bakery";
    let search_name = "Bakery";

    let txn = db.begin().await?;
    _transaction_with_reference(&txn, name1, name2, search_name).await?;
    txn.commit().await?;

    assert_eq!(count_bakeries(&db, search_name).await?, 2);

    drop(db);
    ctx.delete().await;

    Ok(())
}

async fn _transaction_with_reference(
    txn: &DatabaseTransaction<'_>,
    name1: &str,
    name2: &str,
    search_name: &str,
) -> Result<(), Error> {
    insert_bakery(txn, name1, 10.4).await?;
    insert_bakery(txn, name2, 15.0).await?;

    assert_eq!(count_bakeries(txn, search_name).await?, 2);

    Ok(())
}

// [spec:pgorm:sem:conn.tx.guard+2/test]    the implicit path: dropping an uncommitted handle rolls back, and the queued ROLLBACK lands ahead of the connection's next statement
#[pgorm_macros::test]
pub async fn transaction_begin_out_of_scope() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_begin_out_of_scope_test").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    assert_eq!(bakery::Entity::find().all(&db).await?.len(), 0);

    {
        // Transaction begin in this scope
        let txn = db.begin().await?;

        insert_bakery(&txn, "SeaSide Bakery", 10.4).await?;

        assert_eq!(bakery::Entity::find().all(&txn).await?.len(), 1);

        insert_bakery(&txn, "Top Bakery", 15.0).await?;

        assert_eq!(bakery::Entity::find().all(&txn).await?.len(), 2);

        // The scope ended and transaction is dropped without commit
    }

    assert_eq!(bakery::Entity::find().all(&db).await?.len(), 0);

    drop(db);
    ctx.delete().await;

    Ok(())
}

#[pgorm_macros::test]
pub async fn transaction_begin_commit() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_begin_commit_test").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    assert_eq!(bakery::Entity::find().all(&db).await?.len(), 0);

    {
        // Transaction begin in this scope
        let txn = db.begin().await?;

        insert_bakery(&txn, "SeaSide Bakery", 10.4).await?;

        assert_eq!(bakery::Entity::find().all(&txn).await?.len(), 1);

        insert_bakery(&txn, "Top Bakery", 15.0).await?;

        assert_eq!(bakery::Entity::find().all(&txn).await?.len(), 2);

        // Commit changes before the end of scope
        txn.commit().await?;
    }

    assert_eq!(bakery::Entity::find().all(&db).await?.len(), 2);

    drop(db);
    ctx.delete().await;

    Ok(())
}

#[pgorm_macros::test]
pub async fn transaction_error_rollback() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_error_rollback_test").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    assert_eq!(bakery::Entity::find().all(&db).await?.len(), 0);

    {
        let txn = db.begin().await?;

        insert_bakery(&txn, "SeaSide Bakery", 10.4).await?;
        insert_bakery(&txn, "Top Bakery", 15.0).await?;

        assert_eq!(bakery::Entity::find().all(&txn).await?.len(), 2);

        let res = bakery::ActiveModel {
            id: set(1),
            name: set("Duplicated primary key"),
            profit_margin: set(20.0),
        }
        .insert(&txn)
        .await;

        assert!(res.is_err());

        // The scope ended and transaction is dropped without commit
    }

    assert_eq!(bakery::Entity::find().all(&db).await?.len(), 0);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:conn.tx.guard+2/test]    the explicit path: rollback() consumes the handle and awaits the round trip
#[pgorm_macros::test]
pub async fn transaction_explicit_rollback() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_explicit_rollback_txrollback").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    assert_eq!(count_bakeries(&db, "Bakery").await?, 0);

    let txn = db.begin().await?;

    insert_bakery(&txn, "SeaSide Bakery", 10.4).await?;
    insert_bakery(&txn, "Top Bakery", 15.0).await?;

    assert_eq!(count_bakeries(&txn, "Bakery").await?, 2);

    txn.rollback().await.unwrap();

    assert_eq!(count_bakeries(&db, "Bakery").await?, 0);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:conn.tx.guard+2/test]    the ROLLBACK queued by Drop still lands after the connection returns to the pool
#[pgorm_macros::test]
pub async fn transaction_drop_rollback_survives_handback() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_drop_rollback_survives_handback").await;
    create_tables(&ctx.db).await?;

    let base_url =
        std::env::var("DATABASE_URL").expect("Enviroment variable 'DATABASE_URL' not set");
    // One connection only, so the next checkout is guaranteed to be the very
    // connection the dropped transaction queued its ROLLBACK on.
    let pool =
        pgorm::connect_with_builder(common::setup::config(&base_url, ctx.db_name()), |builder| {
            builder.max_size(1)
        })?;

    let mut db = pool.get().await?;
    let pid: i32 = db.query_one("SELECT pg_backend_pid()", &[]).await?.get(0);

    {
        let txn = db.begin().await?;
        insert_bakery(&txn, "Dropped Bakery", 10.4).await?;
        assert_eq!(count_bakeries(&txn, "Bakery").await?, 1);
        // Dropped without commit: Drop enqueues ROLLBACK and returns at once,
        // without awaiting the server's response.
    }

    drop(db);

    let db = pool.get().await?;
    let same_pid: i32 = db.query_one("SELECT pg_backend_pid()", &[]).await?.get(0);
    assert_eq!(
        same_pid, pid,
        "a max_size 1 pool hands the same connection to the next caller"
    );
    assert_eq!(
        count_bakeries(&db, "Bakery").await?,
        0,
        "the queued ROLLBACK is drained before the next caller's work"
    );

    drop(db);
    drop(pool);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:conn.tx.guard+2/test]    a failing COMMIT reaches the caller as Error::Postgres
#[pgorm_macros::test]
pub async fn transaction_commit_failure_maps_error() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_commit_failure_maps_error").await;
    let mut db = ctx.db.get().await?;

    // A deferred constraint is only checked at COMMIT, so the failure can reach
    // no statement other than commit() itself.
    db.batch_execute(
        "CREATE TABLE deferred_probe (id int, CONSTRAINT deferred_probe_key UNIQUE (id) \
         DEFERRABLE INITIALLY DEFERRED)",
    )
    .await?;

    let txn = db.begin().await?;
    txn.execute("INSERT INTO deferred_probe VALUES (1)", &[])
        .await?;
    txn.execute("INSERT INTO deferred_probe VALUES (1)", &[])
        .await?;

    let err = txn
        .commit()
        .await
        .expect_err("the deferred unique constraint must fail the COMMIT");

    match &err {
        Error::Postgres(e) => assert_eq!(
            e.as_db_error().map(|e| e.code()),
            Some(&SqlState::UNIQUE_VIOLATION),
        ),
        other => panic!("expected Error::Postgres, got {other:?}"),
    }

    assert!(
        db.query_all("SELECT id FROM deferred_probe", &[])
            .await?
            .is_empty(),
        "a failed COMMIT leaves nothing behind"
    );

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:req:conn.tx+2/test]    ReadOnly emits both clauses, and the server enforces the access mode
#[pgorm_macros::test]
pub async fn transaction_read_only_txconfig() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_read_only_txconfig").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    let txn = db
        .begin_with(TransactionMode::ReadOnly {
            isolation: Some(IsolationLevel::RepeatableRead),
        })
        .await?;

    assert_eq!(
        show(&txn, "transaction_isolation").await?,
        "repeatable read"
    );
    assert_eq!(show(&txn, "transaction_read_only").await?, "on");

    let err = insert_bakery(&txn, "Read Only Bakery", 10.4)
        .await
        .expect_err("INSERT must be rejected in a read-only transaction");

    assert_read_only_violation(&err);

    txn.rollback().await?;

    assert_eq!(count_bakeries(&db, "Bakery").await?, 0);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:req:conn.tx+2/test]    ReadWrite carries the isolation level, and still commits
#[pgorm_macros::test]
pub async fn transaction_serializable_txconfig() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_serializable_txconfig").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    let txn = db
        .begin_with(TransactionMode::ReadWrite {
            isolation: Some(IsolationLevel::Serializable),
        })
        .await?;

    assert_eq!(show(&txn, "transaction_isolation").await?, "serializable");
    assert_eq!(show(&txn, "transaction_read_only").await?, "off");

    insert_bakery(&txn, "SeaSide Bakery", 10.4).await?;

    assert_eq!(count_bakeries(&txn, "Bakery").await?, 1);

    txn.commit().await?;

    assert_eq!(count_bakeries(&db, "Bakery").await?, 1);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:req:conn.tx+2/test]    Default appends no clause, so the session default stands
#[pgorm_macros::test]
pub async fn transaction_default_inherits_session_txconfig() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_default_inherits_session_txconfig").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    db.batch_execute("SET default_transaction_read_only = on")
        .await?;

    let txn = db.begin_with(TransactionMode::Default).await?;

    assert_eq!(show(&txn, "transaction_read_only").await?, "on");

    let err = insert_bakery(&txn, "Read Only Bakery", 10.4)
        .await
        .expect_err("Default must inherit the session's read-only default");

    assert_read_only_violation(&err);

    txn.rollback().await?;

    db.batch_execute("SET default_transaction_read_only = off")
        .await?;

    assert_eq!(count_bakeries(&db, "Bakery").await?, 0);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:req:conn.tx+2/test]    ReadWrite overrides a read-only session default
#[pgorm_macros::test]
pub async fn transaction_read_write_overrides_default_txconfig() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_read_write_overrides_default_txconfig").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    db.batch_execute("SET default_transaction_read_only = on")
        .await?;

    let txn = db
        .begin_with(TransactionMode::ReadWrite { isolation: None })
        .await?;

    assert_eq!(show(&txn, "transaction_read_only").await?, "off");

    insert_bakery(&txn, "SeaSide Bakery", 10.4).await?;

    txn.commit().await?;

    db.batch_execute("SET default_transaction_read_only = off")
        .await?;

    assert_eq!(count_bakeries(&db, "Bakery").await?, 1);

    drop(db);
    ctx.delete().await;

    Ok(())
}

#[pgorm_macros::test]
pub async fn transaction_with_active_model_behaviour() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_with_active_model_behaviour_test").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    {
        let txn = db.begin().await?;

        assert_eq!(
            cake::ActiveModel {
                name: set("Cake with invalid price"),
                price: set(rust_dec(0)),
                gluten_free: set(false),
                ..Default::default()
            }
            .insert(&txn)
            .await,
            Err(Error::Custom(
                "[before_save] Invalid Price, insert: true".to_owned()
            ))
        );

        assert_eq!(cake::Entity::find().all(&txn).await?.len(), 0);

        assert_eq!(
            cake::ActiveModel {
                name: set("Cake with invalid price"),
                price: set(rust_dec(-10)),
                gluten_free: set(false),
                ..Default::default()
            }
            .insert(&txn)
            .await,
            Err(Error::Custom(
                "[after_save] Invalid Price, insert: true".to_owned()
            ))
        );

        assert_eq!(cake::Entity::find().all(&txn).await?.len(), 1);

        let readonly_cake_1 = cake::ActiveModel {
            name: set("Readonly cake (err_on_before_delete)"),
            price: set(rust_dec(10)),
            gluten_free: set(true),
            ..Default::default()
        }
        .insert(&txn)
        .await?;

        assert_eq!(cake::Entity::find().all(&txn).await?.len(), 2);

        assert_eq!(
            readonly_cake_1.delete(&txn).await.err(),
            Some(Error::Custom(
                "[before_delete] Cannot be deleted".to_owned()
            ))
        );

        assert_eq!(cake::Entity::find().all(&txn).await?.len(), 2);

        let readonly_cake_2 = cake::ActiveModel {
            name: set("Readonly cake (err_on_after_delete)"),
            price: set(rust_dec(10)),
            gluten_free: set(true),
            ..Default::default()
        }
        .insert(&txn)
        .await?;

        assert_eq!(cake::Entity::find().all(&txn).await?.len(), 3);

        assert_eq!(
            readonly_cake_2.delete(&txn).await.err(),
            Some(Error::Custom("[after_delete] Cannot be deleted".to_owned()))
        );

        assert_eq!(cake::Entity::find().all(&txn).await?.len(), 2);
    }

    assert_eq!(cake::Entity::find().all(&db).await?.len(), 0);

    drop(db);
    ctx.delete().await;

    Ok(())
}

#[pgorm_macros::test]
pub async fn transaction_nested() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_nested_test").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    let mut txn = db.begin().await?;

    insert_bakery(&txn, "SeaSide Bakery", 10.4).await?;
    insert_bakery(&txn, "Top Bakery", 15.0).await?;

    assert_eq!(count_bakeries(&txn, "Bakery").await?, 2);

    {
        // Nested transaction (savepoint) that gets committed
        let mut txn2 = txn.begin().await?;

        insert_bakery(&txn2, "Nested Bakery", 88.88).await?;

        assert_eq!(count_bakeries(&txn2, "Bakery").await?, 3);

        {
            // Nested-nested transaction rolled back on drop
            let txn3 = txn2.begin().await?;

            insert_bakery(&txn3, "Rock n Roll Bakery", 28.8).await?;

            assert_eq!(count_bakeries(&txn3, "Bakery").await?, 4);
        }

        assert_eq!(count_bakeries(&txn2, "Bakery").await?, 3);

        {
            // Nested-nested transaction committed
            let txn3 = txn2.begin().await?;

            insert_bakery(&txn3, "Rock n Roll Bakery", 28.8).await?;

            assert_eq!(count_bakeries(&txn3, "Bakery").await?, 4);

            txn3.commit().await?;
        }

        assert_eq!(count_bakeries(&txn2, "Bakery").await?, 4);

        txn2.commit().await?;
    }

    assert_eq!(count_bakeries(&txn, "Bakery").await?, 4);

    {
        // Nested transaction (savepoint) rolled back on drop
        let mut txn2 = txn.begin().await?;

        insert_bakery(&txn2, "Rock n Roll Bakery", 28.8).await?;

        assert_eq!(count_bakeries(&txn2, "Bakery").await?, 5);

        {
            // Nested-nested transaction committed
            let txn3 = txn2.begin().await?;

            insert_bakery(&txn3, "Rock n Roll Bakery", 28.8).await?;

            assert_eq!(count_bakeries(&txn3, "Bakery").await?, 6);

            txn3.commit().await?;
        }

        assert_eq!(count_bakeries(&txn2, "Bakery").await?, 6);

        {
            // Nested-nested transaction rolled back on drop
            let txn3 = txn2.begin().await?;

            insert_bakery(&txn3, "Rock n Roll Bakery", 28.8).await?;

            assert_eq!(count_bakeries(&txn3, "Bakery").await?, 7);
        }

        assert_eq!(count_bakeries(&txn2, "Bakery").await?, 6);
    }

    assert_eq!(count_bakeries(&txn, "Bakery").await?, 4);

    txn.commit().await?;

    assert_eq!(count_bakeries(&db, "Bakery").await?, 4);

    drop(db);
    ctx.delete().await;

    Ok(())
}

fn serializable() -> TransactionMode {
    TransactionMode::ReadWrite {
        isolation: Some(IsolationLevel::Serializable),
    }
}

// [spec:pgorm:sem:conn.tx.closure+1/test]    Ok commits, and the value is returned
#[pgorm_macros::test]
pub async fn transaction_closure_commit_txclosure() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_closure_commit_txclosure").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    let inserted = db
        .transaction(async |txn| {
            insert_bakery(&*txn, "SeaSide Bakery", 10.4).await?;
            insert_bakery(&*txn, "Top Bakery", 15.0).await?;
            count_bakeries(&*txn, "Bakery").await
        })
        .await
        .expect("closure transaction must commit");

    assert_eq!(inserted, 2);
    assert_eq!(count_bakeries(&db, "Bakery").await?, 2);

    let counted = db
        .transaction_with(serializable(), async |txn| {
            count_bakeries(&*txn, "Bakery").await
        })
        .await
        .expect("configured closure transaction must commit");

    assert_eq!(counted, 2);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:conn.tx.closure+1/test]    Err rolls back and is wrapped, not swallowed
#[pgorm_macros::test]
pub async fn transaction_closure_error_txclosure() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_closure_error_txclosure").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    let err = db
        .transaction(async |txn| {
            insert_bakery(&*txn, "SeaSide Bakery", 10.4).await?;
            insert_bakery(&*txn, "Top Bakery", 15.0).await?;

            assert_eq!(count_bakeries(&*txn, "Bakery").await?, 2);

            Err::<(), Error>(Error::Custom("closure said no".to_owned()))
        })
        .await
        .expect_err("closure transaction must not commit");

    match err {
        TransactionError::Transaction(Error::Custom(message)) => {
            assert_eq!(message, "closure said no")
        }
        other => panic!("expected TransactionError::Transaction, got {other:?}"),
    }

    assert_eq!(count_bakeries(&db, "Bakery").await?, 0);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:conn.tx.retry+1/test]    SQLSTATE 40001 raised by a statement is retried
#[pgorm_macros::test]
pub async fn transaction_retry_concurrent_update_txclosure() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_retry_concurrent_update_txclosure").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    insert_bakery(&db, "SeaSide Bakery", 10.4).await?;

    let pool = ctx.db.clone();
    let mut attempts = 0u32;

    let read = db
        .transaction_with_retry(serializable(), 3, async |txn| {
            attempts += 1;

            let row = txn
                .query_one(
                    "SELECT profit_margin FROM bakery WHERE name = $1",
                    &[&"SeaSide Bakery"],
                )
                .await?;
            let margin: f64 = row.get(0);

            if attempts == 1 {
                let mut other = pool.get().await?;
                let other_txn = other.begin().await?;
                other_txn
                    .execute(
                        "UPDATE bakery SET profit_margin = 99.0 WHERE name = $1",
                        &[&"SeaSide Bakery"],
                    )
                    .await?;
                other_txn.commit().await?;
            }

            txn.execute(
                "UPDATE bakery SET profit_margin = $1 WHERE name = $2",
                &[&(margin + 1.0), &"SeaSide Bakery"],
            )
            .await?;

            Ok::<_, Error>(margin)
        })
        .await
        .expect("the retried transaction must eventually commit");

    assert_eq!(attempts, 2);
    assert_eq!(read, 99.0);

    let bakery = Bakery::find().one(&db).await?;
    assert_eq!(bakery.profit_margin, 100.0);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:conn.tx.retry+1/test]    write skew between two serializable transactions
#[pgorm_macros::test]
pub async fn transaction_retry_write_skew_txclosure() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_retry_write_skew_txclosure").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    let pool = ctx.db.clone();
    let mut attempts = 0u32;

    let seen = db
        .transaction_with_retry(serializable(), 3, async |txn| {
            attempts += 1;

            let seen = count_bakeries(&*txn, "Bakery").await?;
            insert_bakery(&*txn, "Top Bakery", 15.0).await?;

            if attempts == 1 {
                let mut other = pool.get().await?;
                let other_txn = other.begin_with(serializable()).await?;
                count_bakeries(&other_txn, "Bakery").await?;
                insert_bakery(&other_txn, "Rival Bakery", 20.0).await?;
                other_txn.commit().await?;
            }

            Ok::<_, Error>(seen)
        })
        .await
        .expect("the retried transaction must eventually commit");

    assert_eq!(attempts, 2);
    assert_eq!(seen, 1);
    assert_eq!(count_bakeries(&db, "Bakery").await?, 2);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:conn.tx.retry+1/test]    a non-retryable error is returned on the first attempt
#[pgorm_macros::test]
pub async fn transaction_retry_non_retryable_txclosure() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_retry_non_retryable_txclosure").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    let mut attempts = 0u32;

    let err = db
        .transaction_with_retry(serializable(), 5, async |txn| {
            attempts += 1;
            insert_bakery(&*txn, "SeaSide Bakery", 10.4).await?;
            Err::<(), Error>(Error::Custom("closure said no".to_owned()))
        })
        .await
        .expect_err("a non-retryable error must not be retried");

    assert_eq!(attempts, 1);
    match err {
        TransactionError::Transaction(Error::Custom(message)) => {
            assert_eq!(message, "closure said no")
        }
        other => panic!("expected TransactionError::Transaction, got {other:?}"),
    }

    assert_eq!(count_bakeries(&db, "Bakery").await?, 0);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:req:conn.tx+2/test]    a savepoint under a configured outer transaction
#[pgorm_macros::test]
pub async fn transaction_nested_under_config_txtests() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_nested_under_config_txtests").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    let mut txn = db.begin_with(serializable()).await?;

    insert_bakery(&txn, "SeaSide Bakery", 10.4).await?;

    {
        let txn2 = txn.begin().await?;

        insert_bakery(&txn2, "Rolled Back Bakery", 88.88).await?;

        assert_eq!(count_bakeries(&txn2, "Bakery").await?, 2);
    }

    assert_eq!(count_bakeries(&txn, "Bakery").await?, 1);

    {
        let txn2 = txn.begin().await?;

        insert_bakery(&txn2, "Top Bakery", 15.0).await?;

        assert_eq!(count_bakeries(&txn2, "Bakery").await?, 2);

        txn2.commit().await?;
    }

    assert_eq!(count_bakeries(&txn, "Bakery").await?, 2);

    txn.commit().await?;

    assert_eq!(count_bakeries(&db, "Bakery").await?, 2);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:req:conn.tx+2/test]    DeferrableSnapshot carries serializable + read only, the one combination DEFERRABLE needs
#[pgorm_macros::test]
pub async fn transaction_deferrable_txtests() -> Result<(), Error> {
    let ctx = TestContext::new("transaction_deferrable_txtests").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    insert_bakery(&db, "SeaSide Bakery", 10.4).await?;

    let txn = db.begin_with(TransactionMode::DeferrableSnapshot).await?;

    assert_eq!(show(&txn, "transaction_isolation").await?, "serializable");
    assert_eq!(show(&txn, "transaction_read_only").await?, "on");
    assert_eq!(show(&txn, "transaction_deferrable").await?, "on");

    assert_eq!(count_bakeries(&txn, "Bakery").await?, 1);

    txn.commit().await?;

    assert_eq!(count_bakeries(&db, "Bakery").await?, 1);

    drop(db);
    ctx.delete().await;

    Ok(())
}
