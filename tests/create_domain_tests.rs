#![allow(unused_imports, dead_code)]

//! `CREATE DOMAIN`, `ALTER DOMAIN` and `DROP DOMAIN` against a live
//! PostgreSQL server, and an entity whose columns are domains.
//!
//! The render tests in pgorm-query hold each statement to libpg_query's
//! `CreateDomainStmt`, `AlterDomainStmt`, `RenameStmt` and `DropStmt`. What
//! only a server settles is what they do: that a domain's default, `NOT NULL`
//! and checks hold every column typed with it, that each alter action changes
//! the domain as it says, that a `NOT VALID` check leaves the values already
//! there until it is validated, that a drop is refused while a column has the
//! type, and what the server refuses that no builder value records. An entity
//! whose columns are domains — over an integer, over an enum, and an array of
//! one — is created from its own definition, written, read and filtered, and a
//! check's refusal reaches the caller as the error PostgreSQL raised.

pub mod common;
pub use common::{TestContext, setup::*};
use pgorm::pgorm_query::{
    ColumnType, ConstraintDrop, Domain, DomainCheck, Expr, Name, Query, Schema, extension::Type,
};
use pgorm::{ConnectionTrait, TransactionTrait, entity::prelude::*};
use pretty_assertions::assert_eq;
use std::sync::Arc;
use tokio_postgres::error::SqlState;

/// A mood: the enum type `cheerful` is a domain over.
mod mood {
    use pgorm::entity::prelude::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
    #[pgorm(rs_type = "String", db_type = "Enum", enum_name = "mood")]
    pub enum Mood {
        #[pgorm(string_value = "sad")]
        Sad,
        #[pgorm(string_value = "calm")]
        Calm,
        #[pgorm(string_value = "glad")]
        Glad,
    }
}

use mood::Mood;

/// `line`, each of whose columns but its key is typed with a domain.
mod line {
    use super::Mood;
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "line")]
    pub struct Model {
        #[pgorm(primary_key, auto_increment = false)]
        pub id: i32,
        #[pgorm(
            column_type = "Domain { name: Name::runtime(\"quantity\"), schema: Some(Name::runtime(\"billing\")), base: Arc::new(ColumnType::Integer) }"
        )]
        pub qty: i32,
        #[pgorm(
            column_type = "Domain { name: Name::runtime(\"cheerful\"), schema: None, base: Arc::new(Mood::db_type().get_column_type().clone()) }"
        )]
        pub mood: Mood,
        #[pgorm(
            column_type = "Array(Arc::new(ColumnType::Domain { name: Name::runtime(\"code\"), schema: None, base: Arc::new(ColumnType::Text) }))"
        )]
        pub codes: Vec<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

fn n(name: &str) -> Name {
    Name::runtime(name)
}

#[track_caller]
fn refused_with(error: &Error, state: &SqlState) {
    match error {
        Error::Postgres(e) => assert_eq!(e.code(), Some(state), "{e}"),
        other => panic!("expected Error::Postgres, got {other:?}"),
    }
}

/// A check's refusal: `23514`, naming the constraint and the domain.
#[track_caller]
fn checked_by(error: &Error, constraint: &str, domain: &str) {
    refused_with(error, &SqlState::CHECK_VIOLATION);
    let Error::Postgres(e) = error else {
        unreachable!("refused_with matched Error::Postgres");
    };
    let db = e.as_db_error().expect("a server error");
    assert_eq!(db.constraint(), Some(constraint), "{e}");
    assert_eq!(db.datatype(), Some(domain), "{e}");
}

/// A statement the server refuses, and the error it refused it with.
async fn refused(db: &impl ConnectionTrait, sql: &str) -> Error {
    db.batch_execute(sql)
        .await
        .expect_err(&format!("the server refuses {sql}"))
}

/// `billing.quantity`, `code` and `cheerful`, each created by the builder
/// over the type the entity's field holds.
async fn domains(db: &impl ConnectionTrait) -> Result<(), Error> {
    db.batch_execute(&Schema::create(n("billing")).to_string())
        .await?;
    let generator = pgorm::Schema::new();
    db.batch_execute(
        &generator
            .create_enum_from_active_enum::<Mood>()?
            .to_string(),
    )
    .await?;
    let quantity = Domain::create((n("billing"), n("quantity")), ColumnType::Integer)
        .default(1)
        .not_null()
        .check(DomainCheck::new(Expr::domain_value().gt(0)).name(n("positive")))
        .to_string();
    assert_eq!(
        quantity,
        [
            r#"CREATE DOMAIN "billing"."quantity" AS integer DEFAULT 1 NOT NULL"#,
            r#"CONSTRAINT "positive" CHECK (VALUE > 0)"#,
        ]
        .join(" ")
    );
    db.batch_execute(&quantity).await?;
    db.batch_execute(
        &Domain::create(n("code"), ColumnType::Text)
            .check(Expr::domain_value().ne(""))
            .to_string(),
    )
    .await?;
    let cheerful = Domain::create(n("cheerful"), Mood::db_type().get_column_type().clone())
        .check(Expr::domain_value().ne("sad"))
        .to_string();
    assert_eq!(
        cheerful,
        r#"CREATE DOMAIN "cheerful" AS mood CHECK (VALUE <> 'sad')"#
    );
    db.batch_execute(&cheerful).await
}

/// Each column of `table` with its type as the server prints it.
async fn columns_of(
    db: &impl ConnectionTrait,
    table: &str,
) -> Result<Vec<(String, String)>, Error> {
    let rows = db
        .query_all(
            "SELECT attname::text, format_type(atttypid, atttypmod) FROM pg_attribute \
             WHERE attrelid = $1::text::regclass AND attnum > 0 AND NOT attisdropped ORDER BY attnum",
            &[&table],
        )
        .await?;
    Ok(rows.iter().map(|row| (row.get(0), row.get(1))).collect())
}

/// A domain's constraints: each one's name, kind and whether it is validated.
async fn constraints_of(
    db: &impl ConnectionTrait,
    domain: &str,
) -> Result<Vec<(String, String, bool)>, Error> {
    let rows = db
        .query_all(
            "SELECT conname::text, contype::text, convalidated FROM pg_constraint \
             WHERE contypid = $1::text::regtype ORDER BY conname COLLATE \"C\"",
            &[&domain],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|row| (row.get(0), row.get(1), row.get(2)))
        .collect())
}

/// One `t` row's `v`, by id.
async fn value_of(db: &impl ConnectionTrait, id: i32) -> Result<Option<i32>, Error> {
    let row = db
        .query_one("SELECT v FROM t WHERE id = $1", &[&id])
        .await?;
    Ok(row.get(0))
}

fn written(id: i32, qty: i32, mood: Mood, codes: &[&str]) -> line::Model {
    line::Model {
        id,
        qty,
        mood,
        codes: codes.iter().map(|code| (*code).to_owned()).collect(),
    }
}

/// The table an entity of domain columns creates names each domain, and the
/// entity writes, reads and filters through them — the domain over an enum
/// compared as its enum, which is the only way PostgreSQL compares one —
/// while each domain's check refuses what it does not hold.
// [spec:pgorm:req:sql.ddl.domain+2/test]    an entity whose columns are domains over an
// integer, over an enum and an array of one creates its table with them, reads, writes and
// filters through them, a predicate on the enum's domain comparing as the enum, and a check's
// refusal arrives as 23514 naming the constraint
// [spec:pgorm:def:sql.types.column-type+11/test]
// [spec:pgorm:sem:entity.traits.column.enum-cast+8/test]
// [spec:pgorm:def:exec.decode.array+2/test]
#[pgorm_macros::test]
async fn an_entity_reads_and_writes_through_domains() -> Result<(), Error> {
    let ctx = TestContext::new("domain_entity_round_trip").await;
    let db = ctx.db.get().await?;
    domains(&db).await?;
    let create = pgorm::Schema::new()
        .create_table_from_entity(line::Entity)
        .to_string();
    assert_eq!(
        create,
        [
            r#"CREATE TABLE "line" ( "id" integer NOT NULL, "qty" billing.quantity NOT NULL,"#,
            r#""mood" cheerful NOT NULL, "codes" code[] NOT NULL, PRIMARY KEY ("id") )"#,
        ]
        .join(" ")
    );
    db.batch_execute(&create).await?;
    assert_eq!(
        columns_of(&db, "line").await?,
        [
            ("id".to_owned(), "integer".to_owned()),
            ("qty".to_owned(), "billing.quantity".to_owned()),
            ("mood".to_owned(), "cheerful".to_owned()),
            ("codes".to_owned(), "code[]".to_owned()),
        ]
    );

    let first = written(1, 5, Mood::Glad, &["A", "B"]);
    first.clone().into_active().insert(&db).await?;
    written(2, 9, Mood::Calm, &[])
        .into_active()
        .insert(&db)
        .await?;
    assert_eq!(line::Entity::find_by_id(1).one(&db).await?, first.clone());

    let large = line::Entity::find()
        .filter(line::Column::Qty.gt(6))
        .all(&db)
        .await?;
    assert_eq!(large.iter().map(|line| line.id).collect::<Vec<_>>(), [2]);

    // PostgreSQL resolves no enum operator for a domain over an enum, even
    // between two values of the domain, so the predicate compares the column
    // and the value as the enum.
    let glad = line::Entity::find()
        .filter(line::Column::Mood.eq(Mood::Glad))
        .all(&db)
        .await?;
    assert_eq!(glad, std::slice::from_ref(&first));

    let zero = written(3, 0, Mood::Glad, &[])
        .into_active()
        .insert(&db)
        .await
        .expect_err("a quantity of zero");
    checked_by(&zero, "positive", "quantity");
    let sad = written(3, 1, Mood::Sad, &[])
        .into_active()
        .insert(&db)
        .await
        .expect_err("a sad mood");
    checked_by(&sad, "cheerful_check", "cheerful");
    let empty = written(3, 1, Mood::Calm, &["A", ""])
        .into_active()
        .insert(&db)
        .await
        .expect_err("an empty code");
    checked_by(&empty, "code_check", "code");
    let mut lowered = first.clone().into_active();
    lowered.qty = set(-1);
    let lowered = lowered.update(&db).await.expect_err("a negative quantity");
    checked_by(&lowered, "positive", "quantity");
    assert_eq!(line::Entity::find_by_id(1).one(&db).await?, first);

    drop(db);
    ctx.delete().await;
    Ok(())
}

/// The ids of the lines `filter` selects, in id order.
async fn ids_where(
    db: &impl ConnectionTrait,
    filter: impl pgorm::pgorm_query::IntoCondition,
) -> Result<Vec<i32>, Error> {
    Ok(line::Entity::find()
        .filter(filter)
        .order_by_asc(line::Column::Id)
        .all(db)
        .await?
        .iter()
        .map(|line| line.id)
        .collect())
}

/// Every value predicate on a column whose domain is over an enum compares
/// it as the enum, the column and the value each cast to it: PostgreSQL has
/// no operator for the domain itself. The value is compared, not written, so
/// the domain's check does not refuse it, and a keyset cursor over the column
/// pages by the enum's order in either direction.
// [spec:pgorm:sem:entity.traits.column.enum-cast+8/test]    every value predicate, the
// column-taking and expression-taking forms and if_null cast a column whose domain is over an
// enum, and its values, to the enum, live
// [spec:pgorm:sem:exec.cursor.keyset+6/test]    a keyset cursor over a column whose domain is
// over an enum pages forwards and backwards by the enum's order
#[pgorm_macros::test]
async fn domain_enum_predicates_compare_as_the_enum() -> Result<(), Error> {
    use line::Column::{Id, Mood as Moods};

    let ctx = TestContext::new("domain_enum_predicates").await;
    let db = ctx.db.get().await?;
    domains(&db).await?;
    db.batch_execute(
        &pgorm::Schema::new()
            .create_table_from_entity(line::Entity)
            .to_string(),
    )
    .await?;
    for (id, mood) in [
        (1, Mood::Calm),
        (2, Mood::Glad),
        (3, Mood::Calm),
        (4, Mood::Glad),
        (5, Mood::Calm),
    ] {
        written(id, 1, mood, &[]).into_active().insert(&db).await?;
    }

    assert_eq!(
        line::Entity::find()
            .select_only()
            .column(Id)
            .filter(Moods.eq(Mood::Glad))
            .as_query()
            .to_string(),
        r#"SELECT "line"."id" FROM "line" WHERE CAST("line"."mood" AS mood) = CAST('glad' AS mood)"#
    );
    let (calm, glad) = (vec![1, 3, 5], vec![2, 4]);
    assert_eq!(ids_where(&db, Moods.eq(Mood::Glad)).await?, glad);
    assert_eq!(ids_where(&db, Moods.ne(Mood::Glad)).await?, calm);
    assert_eq!(ids_where(&db, Moods.gt(Mood::Calm)).await?, glad);
    assert_eq!(
        ids_where(&db, Moods.gte(Mood::Calm)).await?,
        [1, 2, 3, 4, 5]
    );
    assert_eq!(ids_where(&db, Moods.lt(Mood::Glad)).await?, calm);
    assert_eq!(ids_where(&db, Moods.lte(Mood::Calm)).await?, calm);
    assert_eq!(ids_where(&db, Moods.is_in([Mood::Glad])).await?, glad);
    assert_eq!(ids_where(&db, Moods.is_not_in([Mood::Glad])).await?, calm);
    assert_eq!(ids_where(&db, Moods.eq_any([Mood::Calm])).await?, calm);
    assert_eq!(ids_where(&db, Moods.ne_all([Mood::Calm])).await?, glad);
    assert_eq!(
        ids_where(&db, Moods.between(Mood::Sad, Mood::Calm)).await?,
        calm
    );
    assert_eq!(
        ids_where(&db, Moods.not_between(Mood::Sad, Mood::Calm)).await?,
        glad
    );
    assert_eq!(ids_where(&db, Moods.eq_col(Moods)).await?, [1, 2, 3, 4, 5]);
    let glad_enum = Expr::val("glad").cast_as(n("mood"));
    assert_eq!(
        ids_where(&db, Moods.eq_expr(glad_enum.clone())).await?,
        glad
    );
    assert_eq!(
        ids_where(&db, Expr::expr(Moods.if_null(Mood::Calm)).eq(glad_enum)).await?,
        glad
    );
    // The domain's check refuses 'sad' as a value written to it; compared
    // with the column, 'sad' is an enum value that no row holds.
    assert_eq!(
        ids_where(&db, Moods.eq(Mood::Sad)).await?,
        Vec::<i32>::new()
    );

    let mut cursor = line::Entity::find().cursor_by((Moods, Id));
    let page = |models: Vec<line::Model>| models.iter().map(|line| line.id).collect::<Vec<_>>();
    assert_eq!(page(cursor.first(2).all(&db).await?), [1, 3]);
    assert_eq!(
        page(cursor.after((Mood::Calm, 3)).first(2).all(&db).await?),
        [5, 2]
    );
    assert_eq!(
        page(cursor.after((Mood::Glad, 2)).first(2).all(&db).await?),
        [4]
    );
    let mut backwards = line::Entity::find().cursor_by((Moods, Id));
    assert_eq!(
        page(backwards.before((Mood::Glad, 2)).last(2).all(&db).await?),
        [3, 5]
    );
    let mut descending = line::Entity::find().cursor_by(Moods);
    assert_eq!(
        page(descending.desc().after(Mood::Glad).all(&db).await?).len(),
        3
    );

    drop(db);
    ctx.delete().await;
    Ok(())
}

/// PostgreSQL's plan cache does not see a domain's default change: an insert a
/// connection prepared before `SET DEFAULT` keeps writing the default it was
/// planned with, where the same insert prepared afterwards — on another
/// connection, whose statement cache is its own — writes the new one. A
/// check added at the same point holds at once.
// [spec:pgorm:req:sql.ddl.domain+2/test]    an insert prepared before SET DEFAULT keeps the old
// default on its connection while a fresh connection's writes the new one, and a check added
// then holds at once
#[pgorm_macros::test]
async fn a_prepared_insert_keeps_its_planned_default() -> Result<(), Error> {
    let ctx = TestContext::new("domain_default_plan_cache").await;
    let first = ctx.db.get().await?;
    domains(&first).await?;
    first
        .batch_execute(
            &pgorm::Schema::new()
                .create_table_from_entity(line::Entity)
                .to_string(),
        )
        .await?;
    let defaulted = |id: i32| line::ActiveModel {
        id: set(id),
        mood: set(Mood::Calm),
        codes: set(Vec::new()),
        ..Default::default()
    };
    assert_eq!(defaulted(1).insert(&first).await?.qty, 1);

    let set_default = Domain::alter((n("billing"), n("quantity")))
        .set_default(2)
        .to_string();
    assert_eq!(
        set_default,
        r#"ALTER DOMAIN "billing"."quantity" SET DEFAULT 2"#
    );
    first.batch_execute(&set_default).await?;
    assert_eq!(defaulted(2).insert(&first).await?.qty, 1);
    let second = ctx.db.get().await?;
    assert_eq!(defaulted(3).insert(&second).await?.qty, 2);

    first
        .batch_execute(
            &Domain::alter((n("billing"), n("quantity")))
                .add_check(
                    DomainCheck::new(Expr::domain_value().gt(1))
                        .name(n("several"))
                        .not_valid(),
                )
                .to_string(),
        )
        .await?;
    let stale = defaulted(4)
        .insert(&first)
        .await
        .expect_err("the planned default under the new check");
    checked_by(&stale, "several", "quantity");

    drop(first);
    drop(second);
    ctx.delete().await;
    Ok(())
}

/// The server process behind a connection, which names the physical
/// connection the pool lent.
async fn backend(db: &impl ConnectionTrait) -> Result<i32, Error> {
    Ok(db.query_one("SELECT pg_backend_pid()", &[]).await?.get(0))
}

/// How many inserts into `line` the connection's session holds prepared on
/// the server.
async fn prepared_inserts(db: &impl ConnectionTrait) -> Result<i64, Error> {
    Ok(db
        .query_one(
            r#"SELECT count(*) FROM pg_prepared_statements WHERE statement LIKE 'INSERT INTO "line"%'"#,
            &[],
        )
        .await?
        .get(0))
}

/// After `clear_statement_caches`, a connection that already ran an insert
/// writes the domain's new default: one idle at the time as soon as it is
/// lent, and one checked out at the time once it is returned and lent again.
/// The insert it had prepared is closed on the server before it runs anything.
// [spec:pgorm:req:conn.pool.statement-cache.clear/test]    idle and checked-out connections that
// prepared an insert before SET DEFAULT write the new default once next lent, their old statement
// deallocated on the server, while a connection still checked out keeps it
// [spec:pgorm:req:sql.ddl.domain+2/test]    the clear is the remedy for a planned default
#[pgorm_macros::test]
async fn a_cleared_pool_writes_the_new_default() -> Result<(), Error> {
    let ctx = TestContext::new("domain_default_cleared").await;
    let held = ctx.db.get().await?;
    domains(&held).await?;
    held.batch_execute(
        &pgorm::Schema::new()
            .create_table_from_entity(line::Entity)
            .to_string(),
    )
    .await?;
    let defaulted = |id: i32| line::ActiveModel {
        id: set(id),
        mood: set(Mood::Calm),
        codes: set(Vec::new()),
        ..Default::default()
    };

    let idle = ctx.db.get().await?;
    assert_eq!(defaulted(1).insert(&held).await?.qty, 1);
    assert_eq!(defaulted(2).insert(&idle).await?.qty, 1);
    let (held_backend, idle_backend) = (backend(&held).await?, backend(&idle).await?);
    drop(idle);

    held.batch_execute(
        &Domain::alter((n("billing"), n("quantity")))
            .set_default(2)
            .to_string(),
    )
    .await?;
    ctx.db.clear_statement_caches();

    assert_eq!(defaulted(3).insert(&held).await?.qty, 1);
    assert_eq!(prepared_inserts(&held).await?, 1);

    let idle = ctx.db.get().await?;
    assert_eq!(backend(&idle).await?, idle_backend);
    assert_eq!(prepared_inserts(&idle).await?, 0);
    assert_eq!(defaulted(4).insert(&idle).await?.qty, 2);

    drop(held);
    let held = ctx.db.get().await?;
    assert_eq!(backend(&held).await?, held_backend);
    assert_eq!(prepared_inserts(&held).await?, 0);
    assert_eq!(defaulted(5).insert(&held).await?.qty, 2);
    assert_eq!(prepared_inserts(&held).await?, 1);

    drop(held);
    drop(idle);
    ctx.delete().await;
    Ok(())
}

/// Each alter action does what it says to a domain a table's column is typed
/// with: a default fills an insert that leaves the column out, `NOT NULL` is
/// refused over a null and then holds, and a named check is added, renamed and
/// dropped; a domain is renamed with the column still typed by it.
// [spec:pgorm:req:sql.ddl.domain+2/test]    each alter action changes the domain: a default and
// its drop, NOT NULL refused over a null (23502) and then held, a check added, renamed and
// dropped, a rename of the domain; and a missing constraint (42704) or a taken name (42710)
// refused
#[pgorm_macros::test]
async fn each_alter_action_changes_the_domain() -> Result<(), Error> {
    let ctx = TestContext::new("domain_alter_actions").await;
    let db = ctx.db.get().await?;
    db.batch_execute(&Domain::create(n("d"), ColumnType::Integer).to_string())
        .await?;
    db.batch_execute("CREATE TABLE t (id integer PRIMARY KEY, v d)")
        .await?;

    let set_default = Domain::alter(n("d")).set_default(7).to_string();
    assert_eq!(set_default, r#"ALTER DOMAIN "d" SET DEFAULT 7"#);
    db.batch_execute(&set_default).await?;
    db.batch_execute("INSERT INTO t (id) VALUES (1)").await?;
    assert_eq!(value_of(&db, 1).await?, Some(7));
    db.batch_execute(&Domain::alter(n("d")).drop_default().to_string())
        .await?;
    db.batch_execute("INSERT INTO t (id) VALUES (2)").await?;
    assert_eq!(value_of(&db, 2).await?, None);

    let not_null = Domain::alter(n("d")).set_not_null().to_string();
    refused_with(
        &refused(&db, &not_null).await,
        &SqlState::NOT_NULL_VIOLATION,
    );
    db.batch_execute("DELETE FROM t WHERE id = 2").await?;
    db.batch_execute(&not_null).await?;
    let null = refused(&db, "INSERT INTO t (id, v) VALUES (3, NULL)").await;
    refused_with(&null, &SqlState::NOT_NULL_VIOLATION);
    db.batch_execute(&Domain::alter(n("d")).drop_not_null().to_string())
        .await?;
    db.batch_execute("INSERT INTO t (id, v) VALUES (3, NULL)")
        .await?;

    let small = Domain::alter(n("d"))
        .add_check(DomainCheck::new(Expr::domain_value().lt(100)).name(n("small")))
        .to_string();
    db.batch_execute(&small).await?;
    checked_by(
        &refused(&db, "INSERT INTO t VALUES (4, 500)").await,
        "small",
        "d",
    );
    db.batch_execute(
        &Domain::alter(n("d"))
            .add_check(Expr::domain_value().gt(-100))
            .to_string(),
    )
    .await?;
    db.batch_execute(
        &Domain::alter(n("d"))
            .rename_constraint(n("small"), n("tiny"))
            .to_string(),
    )
    .await?;
    assert_eq!(
        constraints_of(&db, "d").await?,
        [
            ("d_check".to_owned(), "c".to_owned(), true),
            ("tiny".to_owned(), "c".to_owned(), true),
        ]
    );
    let taken = Domain::alter(n("d"))
        .rename_constraint(n("tiny"), n("d_check"))
        .to_string();
    refused_with(&refused(&db, &taken).await, &SqlState::DUPLICATE_OBJECT);

    db.batch_execute(&Domain::alter(n("d")).drop_constraint(n("tiny")).to_string())
        .await?;
    let again = Domain::alter(n("d"))
        .drop_constraint(ConstraintDrop::new(n("tiny")).if_exists().restrict())
        .to_string();
    assert_eq!(
        again,
        r#"ALTER DOMAIN "d" DROP CONSTRAINT IF EXISTS "tiny" RESTRICT"#
    );
    db.batch_execute(&again).await?;
    let missing = Domain::alter(n("d")).drop_constraint(n("tiny")).to_string();
    refused_with(&refused(&db, &missing).await, &SqlState::UNDEFINED_OBJECT);
    let missing = Domain::alter(n("d"))
        .validate_constraint(n("tiny"))
        .to_string();
    refused_with(&refused(&db, &missing).await, &SqlState::UNDEFINED_OBJECT);
    db.batch_execute("INSERT INTO t VALUES (4, 500)").await?;

    db.batch_execute(&Domain::alter(n("d")).rename_to(n("e")).to_string())
        .await?;
    assert_eq!(
        columns_of(&db, "t").await?,
        [
            ("id".to_owned(), "integer".to_owned()),
            ("v".to_owned(), "e".to_owned()),
        ]
    );
    db.batch_execute(&Domain::create(n("f"), ColumnType::Text).to_string())
        .await?;
    let taken = Domain::alter(n("e")).rename_to(n("f")).to_string();
    refused_with(&refused(&db, &taken).await, &SqlState::DUPLICATE_OBJECT);
    let missing = Domain::alter(n("d")).drop_default().to_string();
    refused_with(&refused(&db, &missing).await, &SqlState::UNDEFINED_OBJECT);

    drop(db);
    ctx.delete().await;
    Ok(())
}

/// A check added over values that fail it is refused; added `NOT VALID`, it
/// leaves them and holds every value written after it, and validating it is
/// refused until the failing value is gone.
// [spec:pgorm:req:sql.ddl.domain+2/test]    a check over failing values is refused (23514);
// NOT VALID leaves them, holds new values, and VALIDATE CONSTRAINT is refused (23514) until
// they are gone
#[pgorm_macros::test]
async fn a_not_valid_check_leaves_old_values() -> Result<(), Error> {
    let ctx = TestContext::new("domain_not_valid_check").await;
    let db = ctx.db.get().await?;
    db.batch_execute(&Domain::create(n("d"), ColumnType::Integer).to_string())
        .await?;
    db.batch_execute("CREATE TABLE t (id integer PRIMARY KEY, v d)")
        .await?;
    db.batch_execute("INSERT INTO t VALUES (1, -5), (2, 3)")
        .await?;

    let valid = Domain::alter(n("d"))
        .add_check(DomainCheck::new(Expr::domain_value().gt(0)).name(n("positive")))
        .to_string();
    refused_with(&refused(&db, &valid).await, &SqlState::CHECK_VIOLATION);

    let not_valid = Domain::alter(n("d"))
        .add_check(
            DomainCheck::new(Expr::domain_value().gt(0))
                .name(n("positive"))
                .not_valid(),
        )
        .to_string();
    assert_eq!(
        not_valid,
        r#"ALTER DOMAIN "d" ADD CONSTRAINT "positive" CHECK (VALUE > 0) NOT VALID"#
    );
    db.batch_execute(&not_valid).await?;
    assert_eq!(
        constraints_of(&db, "d").await?,
        [("positive".to_owned(), "c".to_owned(), false)]
    );
    checked_by(
        &refused(&db, "INSERT INTO t VALUES (3, -1)").await,
        "positive",
        "d",
    );

    let validate = Domain::alter(n("d"))
        .validate_constraint(n("positive"))
        .to_string();
    assert_eq!(
        validate,
        r#"ALTER DOMAIN "d" VALIDATE CONSTRAINT "positive""#
    );
    refused_with(&refused(&db, &validate).await, &SqlState::CHECK_VIOLATION);
    db.batch_execute("DELETE FROM t WHERE id = 1").await?;
    db.batch_execute(&validate).await?;
    assert_eq!(
        constraints_of(&db, "d").await?,
        [("positive".to_owned(), "c".to_owned(), true)]
    );

    drop(db);
    ctx.delete().await;
    Ok(())
}

/// A drop of a domain a column has is refused under `RESTRICT`, said or
/// unsaid, until it cascades and takes the column; several domains drop in
/// one statement, and a missing one is refused unless the drop says `IF
/// EXISTS`.
// [spec:pgorm:req:sql.ddl.domain+2/test]    a drop of a domain a column is typed with is refused
// under RESTRICT, said or unsaid (2BP01), until CASCADE drops the column; several drop at once,
// and a missing domain is 42704 unless IF EXISTS
#[pgorm_macros::test]
async fn a_drop_is_refused_while_typed_with() -> Result<(), Error> {
    let ctx = TestContext::new("domain_drop_restrict").await;
    let db = ctx.db.get().await?;
    db.batch_execute(&Domain::create(n("d"), ColumnType::Integer).to_string())
        .await?;
    db.batch_execute(&Domain::create(n("e"), ColumnType::Text).to_string())
        .await?;
    db.batch_execute("CREATE TABLE t (id integer PRIMARY KEY, v d)")
        .await?;

    let unsaid = Domain::drop(n("d")).to_string();
    refused_with(
        &refused(&db, &unsaid).await,
        &SqlState::DEPENDENT_OBJECTS_STILL_EXIST,
    );
    let restrict = Domain::drop(n("d")).restrict().to_string();
    assert_eq!(restrict, r#"DROP DOMAIN "d" RESTRICT"#);
    refused_with(
        &refused(&db, &restrict).await,
        &SqlState::DEPENDENT_OBJECTS_STILL_EXIST,
    );

    let cascade = Domain::drop(n("d")).cascade().to_string();
    db.batch_execute(&cascade).await?;
    assert_eq!(
        columns_of(&db, "t").await?,
        [("id".to_owned(), "integer".to_owned())]
    );

    refused_with(
        &refused(&db, &Domain::drop(n("d")).to_string()).await,
        &SqlState::UNDEFINED_OBJECT,
    );
    let both = Domain::drop(n("d")).name(n("e")).if_exists().to_string();
    assert_eq!(both, r#"DROP DOMAIN IF EXISTS "d", "e""#);
    db.batch_execute(&both).await?;
    let left = db
        .query_one(
            "SELECT count(*) FROM pg_type WHERE typname IN ('d', 'e')",
            &[],
        )
        .await?;
    assert_eq!(left.get::<_, i64>(0), 0);

    drop(db);
    ctx.delete().await;
    Ok(())
}

/// What the server refuses, which no builder value can see: a name a type
/// already has, two constraints of one name, a subquery in a check or a
/// default, a collation on a type without one, and a default its base cannot
/// hold.
// [spec:pgorm:req:sql.ddl.domain+2/test]    a taken name (42710), a constraint named twice (42710),
// a subquery in a check or a default (0A000), a COLLATE on integer (42804) and a default its base
// cannot read (22P02) are the server's refusals
#[pgorm_macros::test]
async fn the_server_refuses_what_no_builder_sees() -> Result<(), Error> {
    let ctx = TestContext::new("domain_server_refusals").await;
    let db = ctx.db.get().await?;
    db.batch_execute(&Domain::create(n("d"), ColumnType::Integer).to_string())
        .await?;

    let cases = [
        (
            Domain::create(n("d"), ColumnType::Text).to_string(),
            SqlState::DUPLICATE_OBJECT,
        ),
        (
            Domain::create(n("twice"), ColumnType::Integer)
                .check(DomainCheck::new(Expr::domain_value().gt(0)).name(n("c")))
                .check(DomainCheck::new(Expr::domain_value().lt(9)).name(n("c")))
                .to_string(),
            SqlState::DUPLICATE_OBJECT,
        ),
        (
            Domain::create(n("sub"), ColumnType::Integer)
                .check(Expr::domain_value().in_subquery(Query::select().expr(1).take()))
                .to_string(),
            SqlState::FEATURE_NOT_SUPPORTED,
        ),
        (
            Domain::create(n("sub"), ColumnType::Integer)
                .default(pgorm::pgorm_query::SimpleExpr::SubQuery(
                    None,
                    Box::new(Query::select().expr(1).take().into_sub_query_statement()),
                ))
                .to_string(),
            SqlState::FEATURE_NOT_SUPPORTED,
        ),
        (
            Domain::create(n("collated"), ColumnType::Integer)
                .collate(n("C"))
                .to_string(),
            SqlState::DATATYPE_MISMATCH,
        ),
        (
            Domain::create(n("wrong"), ColumnType::Integer)
                .default("abc")
                .to_string(),
            SqlState::INVALID_TEXT_REPRESENTATION,
        ),
    ];
    for (sql, state) in cases {
        refused_with(&refused(&db, &sql).await, &state);
    }

    drop(db);
    ctx.delete().await;
    Ok(())
}

/// A domain is created inside a transaction, and a rollback takes it away.
// [spec:pgorm:req:sql.ddl.domain+2/test]    a domain created in a transaction is gone when the
// transaction rolls back
#[pgorm_macros::test]
async fn a_domain_rolls_back_with_its_transaction() -> Result<(), Error> {
    let ctx = TestContext::new("domain_rolls_back").await;
    let mut db = ctx.db.get().await?;
    {
        let txn = db.begin().await?;
        txn.batch_execute(&Domain::create(n("d"), ColumnType::Integer).to_string())
            .await?;
        txn.rollback().await?;
    }
    let left = db
        .query_one("SELECT count(*) FROM pg_type WHERE typname = 'd'", &[])
        .await?;
    assert_eq!(left.get::<_, i64>(0), 0);

    drop(db);
    ctx.delete().await;
    Ok(())
}
