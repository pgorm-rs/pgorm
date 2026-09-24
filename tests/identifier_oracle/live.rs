//! The live leg: the server must read a hostile name the way libpg_query does.
//!
//! The structural oracle trusts one parser. Here, for one representative site
//! per parse-node kind, real objects are created with the nastiest names and
//! the builder's own statement is run against them: the object has to exist
//! in the catalogue under exactly the name's bytes, the statement has to reach
//! it, and a sentinel table that a stacked `DROP TABLE sentinel` would remove
//! has to survive. Each test owns a throwaway database named after it.

use pgorm::{
    ConnectionTrait, DatabaseConnection,
    pgorm_query::{
        Asterisk, ColumnDef, Comment, CommonTableExpression, Expr, ForeignKey, Func, Index, Name,
        Query, Table, WindowStatement, WithClause, extension::Type,
    },
};

use super::corpus::NUL_NAME;
use crate::common::TestContext;

/// The names the live leg creates objects with: a quote-closing stacked
/// statement aimed at the sentinel, the f48c6425 exfiltration payload, a
/// comment-and-placeholder name, a string-closing name, a right-to-left
/// override, a case-folded name, and dollar quoting.
pub const NASTY: [&str; 7] = [
    "x\"; DROP TABLE sentinel; --",
    "x\\\" , (SELECT 1) AS \"leak",
    "a b/* $1 */",
    "'; DROP TABLE sentinel; --",
    "\u{202E}kcatta",
    "MixedCase",
    "$$tag$$",
];

/// Quote `name` for this file's own fixture SQL, independently of the code
/// under test.
fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Provision the test's database with the sentinel table in it.
pub async fn open(test: &str) -> (TestContext, DatabaseConnection) {
    let ctx = TestContext::new(test).await;
    let db = ctx
        .db
        .get()
        .await
        .expect("a connection to the test database");
    db.batch_execute("CREATE TABLE sentinel (id integer)")
        .await
        .expect("the sentinel table is created");
    (ctx, db)
}

/// Assert the sentinel survived, then drop the database.
pub async fn close(ctx: TestContext, db: DatabaseConnection) {
    let survived = db
        .query_one(
            "SELECT count(*) FROM pg_class WHERE relname = 'sentinel'",
            &[],
        )
        .await
        .map(|row| row.get::<_, i64>(0));
    drop(db);
    ctx.delete().await;
    assert_eq!(
        survived.ok(),
        Some(1),
        "a name executed as SQL: the sentinel table is gone"
    );
}

/// Every message in `err`'s source chain, joined.
fn error_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut text = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        text.push_str(" <- ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

async fn run(db: &DatabaseConnection, sql: &str) {
    if let Err(err) = db.execute(sql, &[]).await {
        panic!("statement failed: {err}\n    {sql:?}");
    }
}

async fn one_i32(db: &DatabaseConnection, sql: &str) -> i32 {
    match db.query_one(sql, &[]).await {
        Ok(row) => row.get(0),
        Err(err) => panic!("query failed: {err}\n    {sql:?}"),
    }
}

async fn catalogue_count(db: &DatabaseConnection, sql: &str, name: &str) -> i64 {
    match db.query_one(sql, &[&name]).await {
        Ok(row) => row.get(0),
        Err(err) => panic!("catalogue query failed: {err}"),
    }
}

/// `RangeVar.schemaname`, `RangeVar.relname`, `ColumnDef.colname`,
/// `InsertStmt` target columns and `ColumnRef.fields`: a schema, a table and a
/// column all named with the hostile name, created, written and read through
/// the builders.
// [spec:pgorm:req:security.ident-oracle+2/test]
#[tokio::test]
async fn live_relation_schema_and_column_names() {
    let (ctx, db) = open("ident_oracle_live_relation").await;
    for name in NASTY {
        db.batch_execute(&format!("CREATE SCHEMA {}", ident(name)))
            .await
            .expect("the fixture schema is created");
        let n = || Name::runtime(name);
        run(
            &db,
            &Table::create((n(), n()))
                .col(ColumnDef::new(n()).integer())
                .to_string(),
        )
        .await;
        run(
            &db,
            &Query::insert()
                .into_table((n(), n()))
                .columns([n()])
                .values_panic([7.into()])
                .to_string(),
        )
        .await;
        let read = Query::select()
            .column((n(), n()))
            .from((n(), n()))
            .and_where(Expr::col((n(), n())).eq(7))
            .to_string();
        assert_eq!(one_i32(&db, &read).await, 7, "{read:?}");
        let found = catalogue_count(
            &db,
            "SELECT count(*) FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid \
             JOIN pg_namespace s ON s.oid = c.relnamespace \
             WHERE s.nspname = $1 AND c.relname = $1 AND a.attname = $1",
            name,
        )
        .await;
        assert_eq!(found, 1, "schema, table and column not all named {name:?}");
    }
    close(ctx, db).await;
}

/// `ResTarget.name`, `RangeVar.alias.aliasname`, `RangeSubselect.alias` and
/// `CommonTableExpr.ctename`: every alias kind, read back as the server
/// labels it.
// [spec:pgorm:req:security.ident-oracle+2/test]
#[tokio::test]
async fn live_alias_names() {
    let (ctx, db) = open("ident_oracle_live_alias").await;
    db.batch_execute("CREATE TABLE t (c integer); INSERT INTO t VALUES (7)")
        .await
        .expect("the fixture table is created");
    for name in NASTY {
        let n = || Name::runtime(name);
        let projection = Query::select().expr_as(Expr::val(9), n()).to_string();
        let row = db
            .query_one(&projection, &[])
            .await
            .expect("the alias query runs");
        assert_eq!(row.columns()[0].name(), name, "{projection:?}");
        assert_eq!(row.get::<_, i32>(0), 9);

        let table_alias = Query::select()
            .column((n(), Name::runtime("c")))
            .from_as(Name::runtime("t"), n())
            .to_string();
        assert_eq!(one_i32(&db, &table_alias).await, 7, "{table_alias:?}");

        let subquery_alias = Query::select()
            .column((n(), Name::runtime("c")))
            .from_subquery(
                Query::select()
                    .column(Name::runtime("c"))
                    .from(Name::runtime("t"))
                    .take(),
                n(),
            )
            .to_string();
        assert_eq!(one_i32(&db, &subquery_alias).await, 7, "{subquery_alias:?}");

        let cte = CommonTableExpression::new(
            n(),
            Query::select()
                .column(Name::runtime("c"))
                .from(Name::runtime("t"))
                .take(),
        );
        let through_cte = Query::select()
            .column(Asterisk)
            .from(n())
            .with(WithClause::new(cte))
            .to_string();
        assert_eq!(one_i32(&db, &through_cte).await, 7, "{through_cte:?}");
    }
    close(ctx, db).await;
}

/// `FuncCall.funcname` and `TypeCast.type_name`: a function and a domain
/// created under the hostile name, called and cast to through the builders.
// [spec:pgorm:req:security.ident-oracle+2/test]
#[tokio::test]
async fn live_function_and_type_names() {
    let (ctx, db) = open("ident_oracle_live_function_type").await;
    for name in NASTY {
        db.batch_execute(&format!(
            "CREATE FUNCTION {}() RETURNS integer LANGUAGE sql AS 'SELECT 7'; \
             CREATE DOMAIN {} AS integer",
            ident(name),
            ident(name)
        ))
        .await
        .expect("the fixture function and domain are created");
        let n = || Name::runtime(name);
        let call = Query::select().expr(Func::named(n())).to_string();
        assert_eq!(one_i32(&db, &call).await, 7, "{call:?}");
        let cast = Query::select().expr(Expr::val(8).cast_as(n())).to_string();
        assert_eq!(one_i32(&db, &cast).await, 8, "{cast:?}");
        let domain = catalogue_count(
            &db,
            "SELECT count(*) FROM pg_type WHERE typtype = 'd' AND typname = $1",
            name,
        )
        .await;
        assert_eq!(domain, 1, "no domain named {name:?}");
    }
    close(ctx, db).await;
}

/// `WindowDef.name` / `FuncCall.over`: a window defined and referenced under
/// the hostile name.
// [spec:pgorm:req:security.ident-oracle+2/test]
#[tokio::test]
async fn live_window_names() {
    let (ctx, db) = open("ident_oracle_live_window").await;
    db.batch_execute("CREATE TABLE t (c integer); INSERT INTO t VALUES (7), (7)")
        .await
        .expect("the fixture table is created");
    for name in NASTY {
        let n = || Name::runtime(name);
        let sql = Query::select()
            .expr_window_name(Func::count(Expr::col(Name::runtime("c"))), n())
            .from(Name::runtime("t"))
            .window(n(), WindowStatement::partition_by(Name::runtime("c")))
            .limit(1)
            .to_string();
        let count: i64 = db
            .query_one(&sql, &[])
            .await
            .unwrap_or_else(|err| panic!("{err}\n    {sql:?}"))
            .get(0);
        assert_eq!(count, 2, "{sql:?}");
    }
    close(ctx, db).await;
}

/// `IndexStmt.idxname`, `Constraint.conname`, `CreateEnumStmt` type names and
/// labels, and `COMMENT ON` targets: DDL-only names, checked in the catalogue.
// [spec:pgorm:req:security.ident-oracle+2/test]
#[tokio::test]
async fn live_ddl_object_names_and_labels() {
    let (ctx, db) = open("ident_oracle_live_ddl").await;
    db.batch_execute("CREATE TABLE t (c integer PRIMARY KEY); CREATE TABLE r (rc integer UNIQUE)")
        .await
        .expect("the fixture tables are created");
    for name in NASTY {
        let n = || Name::runtime(name);
        run(
            &db,
            &Index::create(Name::runtime("t"), Name::runtime("c"))
                .name(n())
                .to_string(),
        )
        .await;
        let index = catalogue_count(
            &db,
            "SELECT count(*) FROM pg_class WHERE relkind = 'i' AND relname = $1",
            name,
        )
        .await;
        assert_eq!(index, 1, "no index named {name:?}");

        run(
            &db,
            &ForeignKey::create(
                Name::runtime("t"),
                Name::runtime("c"),
                Name::runtime("r"),
                Name::runtime("rc"),
            )
            .name(n())
            .to_string(),
        )
        .await;
        let constraint = catalogue_count(
            &db,
            "SELECT count(*) FROM pg_constraint WHERE contype = 'f' AND conname = $1",
            name,
        )
        .await;
        assert_eq!(constraint, 1, "no foreign key named {name:?}");

        run(&db, &Type::create(n()).as_enum().values([name]).to_string()).await;
        let label = catalogue_count(
            &db,
            "SELECT count(*) FROM pg_enum e JOIN pg_type t ON t.oid = e.enumtypid \
             WHERE t.typname = $1 AND e.enumlabel = $1",
            name,
        )
        .await;
        assert_eq!(label, 1, "no enum type {name:?} with that label");

        run(
            &db,
            &Comment::on_table(Name::runtime("t"), name).to_string(),
        )
        .await;
        run(
            &db,
            &Comment::on_column(Name::runtime("t"), Name::runtime("c"), name).to_string(),
        )
        .await;
        let described = catalogue_count(
            &db,
            "SELECT count(*) FROM pg_description d JOIN pg_class c ON c.oid = d.objoid \
             WHERE c.relname = 't' AND d.description = $1",
            name,
        )
        .await;
        assert_eq!(described, 2, "comment text did not round-trip as {name:?}");

        run(&db, &ForeignKey::drop(Name::runtime("t"), n()).to_string()).await;
        run(&db, &Index::drop(n()).to_string()).await;
        run(&db, &Type::drop(n()).to_string()).await;
    }
    close(ctx, db).await;
}

/// NUL, live, at one representative site: `Table::create` renders the name
/// with its NUL byte, and sending the statement fails in the client's
/// protocol encoder — no statement reaches the server, the connection stays
/// usable, and no table exists afterwards.
// [spec:pgorm:req:security.ident-oracle.nul+2/test]
#[tokio::test]
async fn live_nul_name_never_reaches_the_server() {
    let (ctx, db) = open("ident_oracle_live_nul").await;
    let sql = Table::create(Name::runtime(NUL_NAME))
        .col(ColumnDef::new(Name::runtime("c")).integer())
        .to_string();
    assert!(sql.contains('\0'), "the NUL byte was not rendered: {sql:?}");
    let prepared = db.execute(&sql, &[]).await;
    let simple = db.batch_execute(&sql).await;
    for (route, outcome) in [("extended", prepared.err()), ("simple", simple.err())] {
        let err = outcome.unwrap_or_else(|| panic!("the {route} protocol sent a NUL-bearing name"));
        let chain = error_chain(&err);
        assert!(
            chain.contains("error encoding message to server")
                && chain.contains("string contains embedded null"),
            "the {route} refusal is not the client-side protocol encoder's: {chain}"
        );
    }
    let alive = one_i32(&db, "SELECT 1").await;
    assert_eq!(alive, 1, "the connection did not survive the refusal");
    let tables = catalogue_count(
        &db,
        "SELECT count(*) FROM pg_class WHERE relname LIKE $1",
        "nul%",
    )
    .await;
    assert_eq!(tables, 0, "a NUL-bearing CREATE TABLE reached the server");
    close(ctx, db).await;
}
