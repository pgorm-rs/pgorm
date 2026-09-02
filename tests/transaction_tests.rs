#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, bakery_chain::*, setup::*};
use pgorm::{
    ActiveValue::Set, DatabaseConnection, DatabaseTransaction, IsolationLevel, TransactionError,
    TransactionOptions, TransactionTrait, entity::prelude::*,
};
use pretty_assertions::assert_eq;
use tokio_postgres::error::SqlState;

async fn insert_bakery<C>(db: &C, name: &str, profit_margin: f64) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    bakery::ActiveModel {
        name: Set(name.to_owned()),
        profit_margin: Set(profit_margin),
        ..Default::default()
    }
    .save(db)
    .await?;

    Ok(())
}

async fn count_bakeries<C>(db: &C, search_name: &str) -> Result<usize, DbErr>
where
    C: ConnectionTrait,
{
    Ok(Bakery::find()
        .filter(bakery::Column::Name.contains(search_name))
        .all(db)
        .await?
        .len())
}

#[pgorm_macros::test]
pub async fn transaction() -> Result<(), DbErr> {
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
pub async fn transaction_with_reference() -> Result<(), DbErr> {
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
) -> Result<(), DbErr> {
    insert_bakery(txn, name1, 10.4).await?;
    insert_bakery(txn, name2, 15.0).await?;

    assert_eq!(count_bakeries(txn, search_name).await?, 2);

    Ok(())
}

#[pgorm_macros::test]
pub async fn transaction_begin_out_of_scope() -> Result<(), DbErr> {
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
pub async fn transaction_begin_commit() -> Result<(), DbErr> {
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
pub async fn transaction_error_rollback() -> Result<(), DbErr> {
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
            id: Set(1),
            name: Set("Duplicated primary key".to_owned()),
            profit_margin: Set(20.0),
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

#[pgorm_macros::test]
pub async fn transaction_explicit_rollback() -> Result<(), DbErr> {
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

// [spec:pgorm:req:conn.tx+1/test]    read-only access mode is enforced by the server
#[pgorm_macros::test]
pub async fn transaction_read_only_txconfig() -> Result<(), DbErr> {
    let ctx = TestContext::new("transaction_read_only_txconfig").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    let txn = db
        .begin_with(TransactionOptions {
            read_only: true,
            ..Default::default()
        })
        .await?;

    let err = insert_bakery(&txn, "Read Only Bakery", 10.4)
        .await
        .expect_err("INSERT must be rejected in a read-only transaction");

    match &err {
        DbErr::Postgres(e) => assert_eq!(
            e.as_db_error().map(|e| e.code()),
            Some(&SqlState::READ_ONLY_SQL_TRANSACTION),
        ),
        other => panic!("expected DbErr::Postgres, got {other:?}"),
    }

    txn.rollback().await?;

    assert_eq!(count_bakeries(&db, "Bakery").await?, 0);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:req:conn.tx+1/test]    a configured isolation level still commits
#[pgorm_macros::test]
pub async fn transaction_serializable_txconfig() -> Result<(), DbErr> {
    let ctx = TestContext::new("transaction_serializable_txconfig").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    let txn = db
        .begin_with(TransactionOptions {
            isolation_level: Some(IsolationLevel::Serializable),
            ..Default::default()
        })
        .await?;

    insert_bakery(&txn, "SeaSide Bakery", 10.4).await?;

    assert_eq!(count_bakeries(&txn, "Bakery").await?, 1);

    txn.commit().await?;

    assert_eq!(count_bakeries(&db, "Bakery").await?, 1);

    drop(db);
    ctx.delete().await;

    Ok(())
}

#[pgorm_macros::test]
pub async fn transaction_with_active_model_behaviour() -> Result<(), DbErr> {
    let ctx = TestContext::new("transaction_with_active_model_behaviour_test").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    {
        let txn = db.begin().await?;

        assert_eq!(
            cake::ActiveModel {
                name: Set("Cake with invalid price".to_owned()),
                price: Set(rust_dec(0)),
                gluten_free: Set(false),
                ..Default::default()
            }
            .save(&txn)
            .await,
            Err(DbErr::Custom(
                "[before_save] Invalid Price, insert: true".to_owned()
            ))
        );

        assert_eq!(cake::Entity::find().all(&txn).await?.len(), 0);

        assert_eq!(
            cake::ActiveModel {
                name: Set("Cake with invalid price".to_owned()),
                price: Set(rust_dec(-10)),
                gluten_free: Set(false),
                ..Default::default()
            }
            .save(&txn)
            .await,
            Err(DbErr::Custom(
                "[after_save] Invalid Price, insert: true".to_owned()
            ))
        );

        assert_eq!(cake::Entity::find().all(&txn).await?.len(), 1);

        let readonly_cake_1 = cake::ActiveModel {
            name: Set("Readonly cake (err_on_before_delete)".to_owned()),
            price: Set(rust_dec(10)),
            gluten_free: Set(true),
            ..Default::default()
        }
        .save(&txn)
        .await?;

        assert_eq!(cake::Entity::find().all(&txn).await?.len(), 2);

        assert_eq!(
            readonly_cake_1.delete(&txn).await.err(),
            Some(DbErr::Custom(
                "[before_delete] Cannot be deleted".to_owned()
            ))
        );

        assert_eq!(cake::Entity::find().all(&txn).await?.len(), 2);

        let readonly_cake_2 = cake::ActiveModel {
            name: Set("Readonly cake (err_on_after_delete)".to_owned()),
            price: Set(rust_dec(10)),
            gluten_free: Set(true),
            ..Default::default()
        }
        .save(&txn)
        .await?;

        assert_eq!(cake::Entity::find().all(&txn).await?.len(), 3);

        assert_eq!(
            readonly_cake_2.delete(&txn).await.err(),
            Some(DbErr::Custom("[after_delete] Cannot be deleted".to_owned()))
        );

        assert_eq!(cake::Entity::find().all(&txn).await?.len(), 2);
    }

    assert_eq!(cake::Entity::find().all(&db).await?.len(), 0);

    drop(db);
    ctx.delete().await;

    Ok(())
}

#[pgorm_macros::test]
pub async fn transaction_nested() -> Result<(), DbErr> {
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

fn serializable() -> TransactionOptions {
    TransactionOptions {
        isolation_level: Some(IsolationLevel::Serializable),
        ..Default::default()
    }
}

// [spec:pgorm:sem:conn.tx.closure/test]    Ok commits, and the value is returned
#[pgorm_macros::test]
pub async fn transaction_closure_commit_txclosure() -> Result<(), DbErr> {
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

// [spec:pgorm:sem:conn.tx.closure/test]    Err rolls back and is wrapped, not swallowed
#[pgorm_macros::test]
pub async fn transaction_closure_error_txclosure() -> Result<(), DbErr> {
    let ctx = TestContext::new("transaction_closure_error_txclosure").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    let err = db
        .transaction(async |txn| {
            insert_bakery(&*txn, "SeaSide Bakery", 10.4).await?;
            insert_bakery(&*txn, "Top Bakery", 15.0).await?;

            assert_eq!(count_bakeries(&*txn, "Bakery").await?, 2);

            Err::<(), DbErr>(DbErr::Custom("closure said no".to_owned()))
        })
        .await
        .expect_err("closure transaction must not commit");

    match err {
        TransactionError::Transaction(DbErr::Custom(message)) => {
            assert_eq!(message, "closure said no")
        }
        other => panic!("expected TransactionError::Transaction, got {other:?}"),
    }

    assert_eq!(count_bakeries(&db, "Bakery").await?, 0);

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:sem:conn.tx.retry/test]    SQLSTATE 40001 raised by a statement is retried
#[pgorm_macros::test]
pub async fn transaction_retry_concurrent_update_txclosure() -> Result<(), DbErr> {
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

            Ok::<_, DbErr>(margin)
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

// [spec:pgorm:sem:conn.tx.retry/test]    write skew between two serializable transactions
#[pgorm_macros::test]
pub async fn transaction_retry_write_skew_txclosure() -> Result<(), DbErr> {
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

            Ok::<_, DbErr>(seen)
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

// [spec:pgorm:sem:conn.tx.retry/test]    a non-retryable error is returned on the first attempt
#[pgorm_macros::test]
pub async fn transaction_retry_non_retryable_txclosure() -> Result<(), DbErr> {
    let ctx = TestContext::new("transaction_retry_non_retryable_txclosure").await;
    create_tables(&ctx.db).await?;
    let mut db = ctx.db.get().await?;

    let mut attempts = 0u32;

    let err = db
        .transaction_with_retry(serializable(), 5, async |txn| {
            attempts += 1;
            insert_bakery(&*txn, "SeaSide Bakery", 10.4).await?;
            Err::<(), DbErr>(DbErr::Custom("closure said no".to_owned()))
        })
        .await
        .expect_err("a non-retryable error must not be retried");

    assert_eq!(attempts, 1);
    match err {
        TransactionError::Transaction(DbErr::Custom(message)) => {
            assert_eq!(message, "closure said no")
        }
        other => panic!("expected TransactionError::Transaction, got {other:?}"),
    }

    assert_eq!(count_bakeries(&db, "Bakery").await?, 0);

    drop(db);
    ctx.delete().await;

    Ok(())
}
