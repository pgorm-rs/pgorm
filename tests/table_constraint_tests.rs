#![allow(unused_imports, dead_code)]

//! Unique and primary-key table constraints against a live PostgreSQL server.
//!
//! `IndexConstraint` exists so that every table constraint the builder can
//! embed is one PostgreSQL accepts; the shapes it refuses — a non-unique
//! kind, an ordered, collated, classed or computed key entry, a predicate, an
//! access method — have no method to reach them, which the `compile_fail`
//! doctests on the type hold. What only a server can settle is the other
//! half: that each shape the builder *can* make is created as the constraint
//! it names, and behaves as one. Each case below reads the constraint back
//! out of `pg_constraint` and `pg_index`, and sets the row its key refuses
//! beside the row it admits.

pub mod common;
pub use common::{TestContext, setup::*};
use pgorm::pgorm_query::{ColumnDef, Deferrability, IndexConstraint, Name, Table};
use pgorm::{ConnectionTrait, entity::prelude::*};
use tokio_postgres::error::SqlState;

#[pgorm_macros::test]
async fn main() -> Result<(), Error> {
    let ctx = TestContext::new("table_constraint_tests").await;
    let db = ctx.db.get().await?;

    every_shape_is_created_as_it_names(&db).await?;
    each_key_refuses_what_it_should(&db).await?;
    the_grammar_has_no_other_shape(&db).await?;

    drop(db);
    ctx.delete().await;

    Ok(())
}

fn refused_with(error: &Error, state: &SqlState) {
    match error {
        Error::Postgres(e) => assert_eq!(e.code(), Some(state), "{e}"),
        other => panic!("expected Error::Postgres, got {other:?}"),
    }
}

fn n(name: &str) -> Name {
    Name::runtime(name)
}

/// `CREATE TABLE ledger (a int NOT NULL, b int NOT NULL, c int, d text, …)`
/// carrying one constraint of every shape the builder has: a composite
/// primary key with an `INCLUDE`, a named two-column unique key, a
/// `NULLS NOT DISTINCT` unique key, and a deferrable one.
fn ledger() -> String {
    Table::create(n("ledger"))
        .col(ColumnDef::new(n("a")).integer().not_null())
        .col(ColumnDef::new(n("b")).integer().not_null())
        .col(ColumnDef::new(n("c")).integer())
        .col(ColumnDef::new(n("d")).text())
        .col(ColumnDef::new(n("e")).integer())
        .index(
            IndexConstraint::primary_key(n("a"))
                .col(n("b"))
                .name(n("ledger_pk"))
                .include([n("d")]),
        )
        .index(
            IndexConstraint::unique(n("c"))
                .col(n("d"))
                .name(n("ledger_c_d")),
        )
        .index(IndexConstraint::unique_nulls_not_distinct(n("e")).name(n("ledger_e")))
        .index(
            IndexConstraint::unique(n("b"))
                .name(n("ledger_b"))
                .deferrability(Deferrability::DeferrableInitiallyDeferred),
        )
        .to_string()
}

/// Each constraint as the catalogue holds it: its type, its key columns and
/// included columns by name, whether nulls are distinct, and whether it is
/// deferrable and initially deferred.
// [spec:pgorm:req:sql.ddl.create-table+10/test]    against a live server: every constraint
// shape the builder makes is created, and is the constraint it names
// [spec:pgorm:req:sql.ddl.deferrability+3/test]
async fn every_shape_is_created_as_it_names(db: &DatabaseConnection) -> Result<(), Error> {
    let create = ledger();
    db.batch_execute(&create).await?;

    let rows = db
        .query_all(
            "SELECT c.conname::text, c.contype::text, \
                    (SELECT string_agg(a.attname, ',' ORDER BY k.ord) \
                       FROM unnest(i.indkey[0:i.indnkeyatts - 1]) WITH ORDINALITY k(attnum, ord) \
                       JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = k.attnum), \
                    COALESCE((SELECT string_agg(a.attname, ',' ORDER BY k.ord) \
                       FROM unnest(i.indkey[i.indnkeyatts:]) WITH ORDINALITY k(attnum, ord) \
                       JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = k.attnum), ''), \
                    i.indnullsnotdistinct, c.condeferrable, c.condeferred \
             FROM pg_constraint c JOIN pg_index i ON i.indexrelid = c.conindid \
             WHERE c.conrelid = 'ledger'::regclass ORDER BY c.conname",
            &[],
        )
        .await?;
    let read: Vec<(String, String, String, String, bool, bool, bool)> = rows
        .iter()
        .map(|row| {
            (
                row.get(0),
                row.get(1),
                row.get(2),
                row.get(3),
                row.get(4),
                row.get(5),
                row.get(6),
            )
        })
        .collect();
    let expect = |name: &str, kind: &str, keys: &str, include: &str, nnd, deferrable, deferred| {
        (
            name.to_owned(),
            kind.to_owned(),
            keys.to_owned(),
            include.to_owned(),
            nnd,
            deferrable,
            deferred,
        )
    };
    assert_eq!(
        read,
        [
            expect("ledger_b", "u", "b", "", false, true, true),
            expect("ledger_c_d", "u", "c,d", "", false, false, false),
            expect("ledger_e", "u", "e", "", true, false, false),
            expect("ledger_pk", "p", "a,b", "d", false, false, false),
        ],
        "{create}"
    );

    Ok(())
}

/// Each key refuses the row that repeats it and admits the row its shape
/// says it should: a composite key only a repeat of the whole key, an
/// included column never, a plain unique key any number of nulls, and a
/// `NULLS NOT DISTINCT` one a second null.
// [spec:pgorm:req:sql.ddl.create-table+10/test]    against a live server: each key refuses
// what it should and nothing else
async fn each_key_refuses_what_it_should(db: &DatabaseConnection) -> Result<(), Error> {
    let insert = |a: i32, b: i32, c: Option<i32>, d: &str, e: Option<i32>| {
        let c = c.map_or("NULL".to_owned(), |c| c.to_string());
        let e = e.map_or("NULL".to_owned(), |e| e.to_string());
        format!("INSERT INTO ledger VALUES ({a}, {b}, {c}, '{d}', {e})")
    };

    db.execute(&insert(1, 1, None, "x", None), &[]).await?;
    // Same `a`, another `b`: the composite key is not repeated, and the
    // included `d` is no part of it.
    db.execute(&insert(1, 2, None, "x", Some(1)), &[]).await?;
    // Nulls are distinct under the plain unique key on (c, d).
    db.execute(&insert(2, 3, None, "x", Some(2)), &[]).await?;

    let repeated_key = db.execute(&insert(1, 4, Some(1), "y", Some(3)), &[]).await;
    assert!(repeated_key.is_ok(), "{repeated_key:?}");
    let refused = db
        .execute(&insert(1, 4, Some(2), "z", Some(4)), &[])
        .await
        .expect_err("a repeated primary key");
    refused_with(&refused, &SqlState::UNIQUE_VIOLATION);

    let refused = db
        .execute(&insert(3, 5, Some(1), "y", Some(5)), &[])
        .await
        .expect_err("a repeated (c, d)");
    refused_with(&refused, &SqlState::UNIQUE_VIOLATION);

    let refused = db
        .execute(&insert(3, 6, None, "w", None), &[])
        .await
        .expect_err("a second null under NULLS NOT DISTINCT");
    refused_with(&refused, &SqlState::UNIQUE_VIOLATION);

    Ok(())
}

/// The shapes `IndexConstraint` has no method for are the ones the
/// table-constraint grammar refuses outright (`42601`): a key list with no
/// kind, an ordered, collated, classed or computed key entry, a predicate, an
/// access method, and `NULLS NOT DISTINCT` on a primary key. Each is written
/// raw here, because the builder cannot write it at all; the control beside
/// them is the same table with a key the grammar takes.
// [spec:pgorm:req:sql.ddl.create-table+10/test]    against a live server: the table-constraint
// shapes the builder cannot express are the ones PostgreSQL refuses
async fn the_grammar_has_no_other_shape(db: &DatabaseConnection) -> Result<(), Error> {
    for constraint in [
        "CONSTRAINT r (k)",
        "UNIQUE (k DESC)",
        "UNIQUE (k ASC)",
        "UNIQUE (t text_pattern_ops)",
        r#"UNIQUE (t COLLATE "C")"#,
        "UNIQUE ((lower(t)))",
        "UNIQUE (k) WHERE (k > 0)",
        "UNIQUE USING hash (k)",
        "PRIMARY KEY NULLS NOT DISTINCT (k)",
    ] {
        let create = format!("CREATE TABLE refused (k integer, t text, {constraint})");
        let refused = db.batch_execute(&create).await.expect_err(&create);
        refused_with(&refused, &SqlState::SYNTAX_ERROR);
    }
    db.batch_execute("CREATE TABLE refused (k integer, t text, UNIQUE (k))")
        .await?;

    Ok(())
}
