use pgorm_migration::pgorm::{Config, ConnectionTrait, DatabasePool, DbErr, types::ToSql};

const NO_PARAMS: [&(dyn ToSql + Sync); 0] = [];

fn config(base_url: &str, db_name: &str) -> Config {
    format!("{base_url}/{db_name}")
        .parse()
        .expect("DATABASE_URL is not a valid PostgreSQL connection string")
}

/// A throwaway database, dropped and recreated on construction.
pub struct TestContext {
    base_url: String,
    db_name: String,
    pub db: DatabasePool,
}

impl TestContext {
    pub async fn new(db_name: &str) -> Self {
        let base_url =
            std::env::var("DATABASE_URL").expect("Environment variable 'DATABASE_URL' not set");

        let maintenance = pgorm_migration::pgorm::connect(config(&base_url, "postgres"));
        let conn = maintenance.get().await.expect("connect to maintenance db");

        conn.execute_raw(
            &format!("DROP DATABASE IF EXISTS \"{db_name}\" WITH (FORCE);"),
            NO_PARAMS,
        )
        .await
        .expect("drop test database");

        conn.execute_raw(&format!("CREATE DATABASE \"{db_name}\";"), NO_PARAMS)
            .await
            .expect("create test database");

        let db = pgorm_migration::pgorm::connect(config(&base_url, db_name));

        Self {
            base_url,
            db_name: db_name.to_owned(),
            db,
        }
    }

    pub async fn delete(&self) {
        let maintenance = pgorm_migration::pgorm::connect(config(&self.base_url, "postgres"));
        let conn = maintenance.get().await.expect("connect to maintenance db");
        let _ = conn
            .execute_raw(
                &format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE);", self.db_name),
                NO_PARAMS,
            )
            .await;
    }
}

pub async fn has_table(db: &DatabasePool, name: &str) -> Result<bool, DbErr> {
    let conn = db.get().await?;
    let params: [&(dyn ToSql + Sync); 1] = [&name];
    let row = conn
        .query_one("SELECT to_regclass($1) IS NOT NULL", &params)
        .await?;
    Ok(row.get(0))
}

pub async fn has_index(db: &DatabasePool, table: &str, index: &str) -> Result<bool, DbErr> {
    let conn = db.get().await?;
    let params: [&(dyn ToSql + Sync); 2] = [&table, &index];
    let row = conn
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE tablename = $1 AND indexname = $2)",
            &params,
        )
        .await?;
    Ok(row.get(0))
}

pub async fn count_rows(db: &DatabasePool, table: &str) -> Result<i64, DbErr> {
    let conn = db.get().await?;
    let row = conn
        .query_one(&format!("SELECT COUNT(*) FROM \"{table}\""), &[])
        .await?;
    Ok(row.get(0))
}
