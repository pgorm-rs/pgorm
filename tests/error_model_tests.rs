#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, bakery_chain::*, setup::*};

use pgorm::{
    ColumnFromStrErr, ConnectionTrait, DbErr, LoaderTrait, RuntimeErr, entity::prelude::*,
};
use pgorm_pool::{PoolError, Runtime, TimeoutType};
use pretty_assertions::assert_eq;
use std::{error::Error as StdError, fmt, str::FromStr, time::Duration};
use tokio_postgres::error::SqlState;

#[derive(Debug)]
struct Boom(&'static str);

impl fmt::Display for Boom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl StdError for Boom {}

// [spec:pgorm:def:error.model+1/test]    the variants pgorm constructs itself, and how each renders
#[test]
fn db_err_variants_render_expected_messages() {
    let cases: Vec<(DbErr, &str)> = vec![
        (
            DbErr::TryIntoErr {
                from: "i64",
                into: "u8",
                source: Box::new(Boom("out of range")),
            },
            "Error converting `i64` into `u8`: out of range",
        ),
        (
            DbErr::Conn(RuntimeErr::Internal("no socket".to_owned())),
            "Connection Error: no socket",
        ),
        (
            DbErr::Exec(RuntimeErr::Internal("bad plan".to_owned())),
            "Execution Error: bad plan",
        ),
        (
            DbErr::Query(RuntimeErr::Internal("bad filter".to_owned())),
            "Query Error: bad filter",
        ),
        (
            DbErr::ConvertFromU64("Uuid"),
            "Type 'Uuid' cannot be converted from u64",
        ),
        (DbErr::UnpackInsertId, "Failed to unpack last_insert_id"),
        (
            DbErr::UpdateGetPrimaryKey,
            "Failed to get primary key from model",
        ),
        (
            DbErr::AttrNotSet("name".to_owned()),
            "Attribute name is NotSet",
        ),
        (
            DbErr::Type("not a date".to_owned()),
            "Type Error: not a date",
        ),
        (
            DbErr::Json("not an object".to_owned()),
            "Json Error: not an object",
        ),
        (
            DbErr::RecordNotFound,
            "No records were returned for the given query",
        ),
        (DbErr::RecordNotInserted, "None of the records are inserted"),
        (DbErr::RecordNotUpdated, "None of the records are updated"),
        (DbErr::Custom("boom".to_owned()), "Custom Error: boom"),
    ];

    for (err, rendered) in cases {
        assert_eq!(err.to_string(), rendered);
    }
}

// [spec:pgorm:def:error.model+1/test]    PartialEq/Eq compare rendered messages, not payloads
#[test]
fn db_err_eq_compares_rendered_messages() {
    fn assert_is_eq<T: Eq>() {}
    assert_is_eq::<DbErr>();

    let from_io = DbErr::TryIntoErr {
        from: "i64",
        into: "u8",
        source: Box::new(std::io::Error::other("out of range")),
    };
    let from_boom = DbErr::TryIntoErr {
        from: "i64",
        into: "u8",
        source: Box::new(Boom("out of range")),
    };
    assert_eq!(
        from_io, from_boom,
        "distinct payloads with identical rendered messages compare equal"
    );

    assert_ne!(DbErr::Custom("a".to_owned()), DbErr::Custom("b".to_owned()));
    assert_ne!(DbErr::RecordNotFound, DbErr::RecordNotInserted);
    assert_eq!(DbErr::RecordNotFound, DbErr::RecordNotFound);
}

// [spec:pgorm:def:error.model+1/test]    ColumnFromStrErr covers FromStr failures on entity columns
#[test]
fn column_from_str_err_reports_bad_input() {
    assert!(matches!(
        cake::Column::from_str("gluten_free"),
        Ok(cake::Column::GlutenFree)
    ));

    let err: ColumnFromStrErr =
        cake::Column::from_str("not_a_column").expect_err("there is no such column");
    assert_eq!(err.0, "not_a_column");
    assert_eq!(
        err.to_string(),
        r#"Failed to match "not_a_column" as Column"#
    );
}

// [spec:pgorm:def:error.model.runtime+1/test]    Internal is the only variant, and the payload of Conn/Exec/Query
#[test]
fn runtime_err_wraps_internal_message() {
    let runtime = RuntimeErr::Internal("boom".to_owned());
    // Irrefutable: this only compiles while `Internal` is the sole variant.
    let RuntimeErr::Internal(message) = &runtime;
    assert_eq!(message, "boom");
    assert_eq!(runtime.to_string(), "boom");

    for err in [
        DbErr::Conn(RuntimeErr::Internal("boom".to_owned())),
        DbErr::Exec(RuntimeErr::Internal("boom".to_owned())),
        DbErr::Query(RuntimeErr::Internal("boom".to_owned())),
    ] {
        let (DbErr::Conn(inner) | DbErr::Exec(inner) | DbErr::Query(inner)) = &err else {
            panic!("expected a RuntimeErr-carrying variant, got {err:?}");
        };
        let RuntimeErr::Internal(message) = inner;
        assert_eq!(message, "boom");
        assert!(
            StdError::source(&err).is_some(),
            "the RuntimeErr is the error source: {err:?}"
        );
    }
}

// [spec:pgorm:def:error.model.runtime+1/test]    a real internal failure arrives as Query(Internal(..))
#[pgorm_macros::test]
async fn query_err_surfaces_through_loader_misuse() -> Result<(), DbErr> {
    let ctx = TestContext::new("error_model_runtime_errmodel").await;
    let db = ctx.db.get().await?;

    // `cake -> lineitem` is HasMany, so asking the loader for one is a pgorm-internal
    // misuse, reported through the crate-private `query_err` helper.
    let err = Vec::<cake::Model>::new()
        .load_one(Lineitem, &db)
        .await
        .expect_err("cake has many lineitems");

    let DbErr::Query(RuntimeErr::Internal(message)) = &err else {
        panic!("expected DbErr::Query(RuntimeErr::Internal(..)), got {err:?}");
    };
    assert_eq!(message, "Relation is HasMany instead of HasOne");
    assert_eq!(
        err.to_string(),
        "Query Error: Relation is HasMany instead of HasOne"
    );

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:def:error.model+1/test]    ConnectionTrait failures arrive as Postgres, rendering the server detail
#[pgorm_macros::test]
async fn db_err_postgres_carries_server_detail() -> Result<(), DbErr> {
    let ctx = TestContext::new("error_model_postgres_errmodel").await;
    let db = ctx.db.get().await?;

    let err = db
        .query_one("SELECT id FROM absent_table", &[])
        .await
        .expect_err("there is no such table");

    let DbErr::Postgres(driver) = &err else {
        panic!("expected DbErr::Postgres, got {err:?}");
    };
    let detail = driver
        .as_db_error()
        .expect("the server reported an error detail");
    assert_eq!(*detail.code(), SqlState::UNDEFINED_TABLE);

    let rendered = err.to_string();
    assert!(rendered.starts_with("Postgres Error:"), "{rendered}");
    // The server-side `DbError` is rendered (debug-formatted, hence the escaped quotes).
    assert!(
        rendered.contains(&format!("message: {:?}", detail.message())),
        "the Display carries the server detail: {rendered}"
    );
    assert!(rendered.contains("SqlState(E42P01)"), "{rendered}");

    // The other ConnectionTrait entry points converge on the same variant.
    for other in [
        db.execute("INSERT INTO absent_table (id) VALUES (1)", &[])
            .await
            .map(|_| ())
            .expect_err("execute fails the same way"),
        db.batch_execute("SELECT id FROM absent_table")
            .await
            .expect_err("batch_execute fails the same way"),
        db.query_all("SELECT id FROM absent_table", &[])
            .await
            .map(|_| ())
            .expect_err("query_all fails the same way"),
    ] {
        assert!(matches!(other, DbErr::Postgres(_)), "{other:?}");
    }

    drop(db);
    ctx.delete().await;

    Ok(())
}

// [spec:pgorm:def:error.model+1/test]    DatabasePool::get surfaces pool exhaustion as DbErr::Pool
#[pgorm_macros::test]
async fn db_err_pool_from_acquisition_timeout() -> Result<(), DbErr> {
    let db_name = "error_model_pool_errmodel";
    let ctx = TestContext::new(db_name).await;
    let base_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is set by TestContext");

    let pool = pgorm::connect_with_builder(config(&base_url, db_name), |builder| {
        builder
            .max_size(1)
            .wait_timeout(Some(Duration::from_millis(100)))
            .runtime(Runtime::Tokio1)
    });

    let held = pool.get().await?;
    let err = pool
        .get()
        .await
        .expect_err("the pool's only connection is checked out");

    assert!(
        matches!(err, DbErr::Pool(PoolError::Timeout(TimeoutType::Wait))),
        "expected a wait timeout, got {err:?}"
    );
    assert!(err.to_string().starts_with("Pool Error:"), "{err}");

    drop(held);
    let recovered = pool.get().await?;
    drop(recovered);

    ctx.delete().await;

    Ok(())
}
