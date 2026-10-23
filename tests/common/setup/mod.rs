use pgorm::{
    ColumnTrait, ColumnType, ConnectionTrait, DatabasePool, EntityTrait, Error, Iterable, Schema,
};
use pgorm_query::{
    Name, QueryBuilder, TableCreateStatement,
    extension::{Type, TypeCreateStatement},
};
use pretty_assertions::assert_eq;

/// `scoped_db_name` names the database or role a test of this label owns: the
/// name carries this process and its place among the names the process
/// minted, so no other run on the server, from this checkout or any other, can
/// hold it.
pub use super::run_scope::{config, scoped_name as scoped_db_name};

pub async fn setup(base_url: &str, db_name: &str) -> DatabasePool {
    super::run_scope::create_database(base_url, db_name)
        .await
        .map_err(|err| Error::Custom(format!("test database {db_name:?} was not created: {err}")))
        .expect("the test database is created");

    pgorm::connect(config(base_url, db_name))
}

/// Drop the database a finished test owned.
///
/// `TestContext::delete` drops the pool holding connections to it first, so
/// `FORCE` has nothing left to evict; it stays as a backstop for a checkout the
/// test leaked. A database that survives every attempt is reported rather than
/// failed on: once this process has exited, the next run's sweep reclaims it.
pub async fn tear_down(base_url: &str, db_name: &str) {
    if let Err(err) = super::run_scope::drop_database(base_url, db_name).await {
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
        let drop_type_stmt = Type::drop(Name::clone(name))
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
        schema.create_table_from_entity(entity)?.to_string(),
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
