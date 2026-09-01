use pgorm::{
    ColumnTrait, ColumnType, ConnectionTrait, DatabasePool, DbErr, EntityTrait, Iterable, Schema,
};
use pgorm_query::{
    QueryBuilder, SeaRc, TableCreateStatement,
    extension::{Type, TypeCreateStatement},
};
use pretty_assertions::assert_eq;
use tokio_postgres::Config;

const NO_PARAMS: [&(dyn tokio_postgres::types::ToSql + Sync); 0] = [];

pub fn config(base_url: &str, db_name: &str) -> Config {
    format!("{base_url}/{db_name}")
        .parse()
        .expect("DATABASE_URL is not a valid PostgreSQL connection string")
}

pub async fn setup(base_url: &str, db_name: &str) -> DatabasePool {
    let maintenance = pgorm::connect(config(base_url, "postgres"));
    let conn = maintenance.get().await.unwrap();

    let _drop_db_result = conn
        .execute_raw(
            &format!("DROP DATABASE IF EXISTS \"{db_name}\" WITH (FORCE);"),
            NO_PARAMS,
        )
        .await;

    let _create_db_result = conn
        .execute_raw(&format!("CREATE DATABASE \"{db_name}\";"), NO_PARAMS)
        .await;

    pgorm::connect(config(base_url, db_name))
}

pub async fn tear_down(base_url: &str, db_name: &str) {
    let maintenance = pgorm::connect(config(base_url, "postgres"));
    let conn = maintenance.get().await.unwrap();

    let _ = conn
        .execute_raw(
            &format!("DROP DATABASE IF EXISTS \"{db_name}\" WITH (FORCE);"),
            NO_PARAMS,
        )
        .await;
}

pub async fn create_enum<C, E>(
    db: &C,
    creates: &[TypeCreateStatement],
    entity: E,
) -> Result<(), DbErr>
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
        let drop_type_stmt = Type::drop()
            .name(SeaRc::clone(name))
            .if_exists()
            .cascade()
            .to_owned();
        db.execute(&drop_type_stmt.to_string(QueryBuilder), &[])
            .await?;
    }

    let expect_stmts: Vec<String> = creates
        .iter()
        .map(|stmt| stmt.to_string(QueryBuilder))
        .collect();
    let schema = Schema::new();
    let create_from_entity_stmts: Vec<String> = schema
        .create_enum_from_entity(entity)
        .iter()
        .map(|stmt| stmt.to_string(QueryBuilder))
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
) -> Result<u64, DbErr>
where
    C: ConnectionTrait,
    E: EntityTrait,
{
    let schema = Schema::new();
    assert_eq!(
        schema.create_table_from_entity(entity).build(QueryBuilder),
        create.build(QueryBuilder)
    );

    create_table_without_asserts(db, create).await
}

pub async fn create_table_without_asserts<C>(
    db: &C,
    create: &TableCreateStatement,
) -> Result<u64, DbErr>
where
    C: ConnectionTrait,
{
    db.execute(&create.build(QueryBuilder), &[]).await
}

pub fn rust_dec<T: ToString>(v: T) -> rust_decimal::Decimal {
    use std::str::FromStr;
    rust_decimal::Decimal::from_str(&v.to_string()).unwrap()
}
