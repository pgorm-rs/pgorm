use std::{future::Future, time::Duration};

use pgorm::{
    ColumnTrait, ColumnType, ConnectionTrait, DatabasePool, EntityTrait, Error, Iterable, Schema,
};
use pgorm_query::{
    QueryBuilder, SharedIden, TableCreateStatement,
    extension::{Type, TypeCreateStatement},
};
use pretty_assertions::assert_eq;
use tokio_postgres::{Config, error::SqlState};

const NO_PARAMS: [&(dyn tokio_postgres::types::ToSql + Sync); 0] = [];

/// How many times a provisioning step is re-attempted before the harness calls
/// the database unobtainable.
const DDL_ATTEMPTS: u32 = 8;

/// Pause before the second attempt; each further attempt waits a multiple of
/// it, so eight attempts span a little under a second and a half.
const DDL_BACKOFF: Duration = Duration::from_millis(50);

/// The longest identifier PostgreSQL stores without truncating it.
const MAX_IDENTIFIER_LEN: usize = 63;

pub fn config(base_url: &str, db_name: &str) -> Config {
    format!("{base_url}/{db_name}")
        .parse()
        .expect("DATABASE_URL is not a valid PostgreSQL connection string")
}

/// The database a test of this name owns *in this checkout*.
///
/// A test function name alone is not unique on the server: several worktrees
/// build the same test binaries and point them at one PostgreSQL instance, so
/// two checkouts would provision, use and force-drop a single database — each
/// one's teardown evicting the other's connections. The suffix is derived from
/// the checkout path, which separates concurrent worktrees while keeping the
/// name stable across runs of the same one, so a database orphaned by a
/// cancelled run is reclaimed by the next run rather than accumulating.
pub fn scoped_db_name(test_name: &str) -> String {
    let suffix = format!("_{:08x}", fnv1a(env!("CARGO_MANIFEST_DIR").as_bytes()));
    let mut name = test_name.to_owned();
    name.truncate(MAX_IDENTIFIER_LEN - suffix.len());
    name.push_str(&suffix);
    name
}

fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Whether an error describes a race with another session rather than a
/// statement the server refused on its merits.
///
/// Each state here was observed by racing `CREATE`/`DROP DATABASE` against an
/// occupied database: a `DROP` that cannot evict its occupants reports
/// `OBJECT_IN_USE`, which leaves the old database in place and so makes the
/// following `CREATE` report `DUPLICATE_DATABASE`, and two sessions dropping
/// one database leave the loser with `UNDEFINED_DATABASE`.
fn is_contended(err: &Error) -> bool {
    let Error::Postgres(err) = err else {
        return false;
    };
    matches!(
        err.code(),
        Some(&SqlState::OBJECT_IN_USE)
            | Some(&SqlState::DUPLICATE_DATABASE)
            | Some(&SqlState::UNDEFINED_DATABASE)
    )
}

/// Re-run `step` while it fails on a contended state, waiting longer each time.
async fn retrying<F, Fut, T>(mut step: F) -> Result<T, Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    let mut attempt = 1;
    loop {
        match step().await {
            Err(err) if attempt < DDL_ATTEMPTS && is_contended(&err) => {
                tokio::time::sleep(DDL_BACKOFF * attempt).await;
                attempt += 1;
            }
            outcome => return outcome,
        }
    }
}

async fn drop_database(maintenance: &DatabasePool, db_name: &str) -> Result<u64, Error> {
    maintenance
        .get()
        .await?
        .execute_raw(
            &format!("DROP DATABASE IF EXISTS \"{db_name}\" WITH (FORCE);"),
            NO_PARAMS,
        )
        .await
}

/// Discard whatever an earlier run left under this name, then create the
/// database the test is about to use.
///
/// The two statements retry as a pair: a `DROP` that lost its race is the
/// reason the `CREATE` finds the name taken, so re-attempting the `CREATE`
/// alone could never recover. Taking the maintenance connection afresh each
/// time keeps one terminated backend from poisoning every attempt.
async fn provision(maintenance: &DatabasePool, db_name: &str) -> Result<(), Error> {
    drop_database(maintenance, db_name).await?;
    maintenance
        .get()
        .await?
        .execute_raw(&format!("CREATE DATABASE \"{db_name}\";"), NO_PARAMS)
        .await?;
    Ok(())
}

pub async fn setup(base_url: &str, db_name: &str) -> DatabasePool {
    let maintenance = pgorm::connect(config(base_url, "postgres"));

    retrying(|| provision(&maintenance, db_name))
        .await
        .map_err(|err| {
            Error::Custom(format!(
                "test database {db_name:?} was not provisioned in {DDL_ATTEMPTS} attempts: {err}"
            ))
        })
        .expect("the test database is provisioned");

    pgorm::connect(config(base_url, db_name))
}

/// Drop the database a finished test owned.
///
/// `TestContext::delete` drops the pool holding connections to it first, so
/// `FORCE` has nothing left to evict; it stays as a backstop for a checkout the
/// test leaked. A database that survives every attempt is reported rather than
/// failed on — the next run of this worktree provisions the same name and
/// reclaims it.
pub async fn tear_down(base_url: &str, db_name: &str) {
    let maintenance = pgorm::connect(config(base_url, "postgres"));

    if let Err(err) = retrying(|| drop_database(&maintenance, db_name)).await {
        tracing::warn!("test database {db_name:?} outlived its test: {err}");
    }
}

pub async fn create_enum<C, E>(
    db: &C,
    creates: &[TypeCreateStatement],
    entity: E,
) -> Result<(), Error>
where
    C: ConnectionTrait,
    E: EntityTrait,
{
    for col in E::Column::iter() {
        let col_def = col.def();
        let col_type = col_def.get_column_type();
        if !matches!(col_type, ColumnType::Enum { .. }) {
            continue;
        }
        let name = match col_type {
            ColumnType::Enum { name, .. } => name,
            _ => unreachable!(),
        };
        let drop_type_stmt = Type::drop(SharedIden::clone(name))
            .if_exists()
            .cascade()
            .to_owned();
        db.execute(&drop_type_stmt.to_string(), &[]).await?;
    }

    let expect_stmts: Vec<String> = creates.iter().map(|stmt| stmt.to_string()).collect();
    let schema = Schema::new();
    let create_from_entity_stmts: Vec<String> = schema
        .create_enum_from_entity(entity)
        .iter()
        .map(|stmt| stmt.to_string())
        .collect();

    assert_eq!(expect_stmts, create_from_entity_stmts);

    for stmt in expect_stmts {
        db.execute(&stmt, &[]).await?;
    }

    Ok(())
}

pub async fn create_table<C, E>(
    db: &C,
    create: &TableCreateStatement,
    entity: E,
) -> Result<u64, Error>
where
    C: ConnectionTrait,
    E: EntityTrait,
{
    let schema = Schema::new();
    assert_eq!(
        schema.create_table_from_entity(entity).to_string(),
        create.to_string()
    );

    create_table_without_asserts(db, create).await
}

pub async fn create_table_without_asserts<C>(
    db: &C,
    create: &TableCreateStatement,
) -> Result<u64, Error>
where
    C: ConnectionTrait,
{
    db.execute(&create.to_string(), &[]).await
}

pub fn rust_dec<T: ToString>(v: T) -> rust_decimal::Decimal {
    use std::str::FromStr;
    rust_decimal::Decimal::from_str(&v.to_string()).unwrap()
}
