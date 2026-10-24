use pgorm_migration::pgorm::{ConnectionTrait, DatabasePool, Error, types::ToSql};

use pgorm_test_support::{config, create_database, drop_database, scoped_name};

/// A throwaway database of this process's own, created on construction under
/// a name no other run can hold (`pgorm_test_support`).
pub struct TestContext {
    base_url: String,
    db_name: String,
    pub db: DatabasePool,
}

impl TestContext {
    pub async fn new(test_name: &str) -> Self {
        dotenvy::from_filename(".env.local").ok();
        dotenvy::from_filename(".env").ok();

        let base_url =
            std::env::var("DATABASE_URL").expect("Environment variable 'DATABASE_URL' not set");
        let db_name = scoped_name(test_name);

        create_database(&base_url, &db_name)
            .await
            .expect("create test database");

        let db = pgorm_migration::pgorm::connect(config(&base_url, &db_name));

        Self {
            base_url,
            db_name,
            db,
        }
    }

    /// A second, independent pool against the same database — what a second
    /// deploy replica racing this one would hold.
    pub fn second_pool(&self) -> DatabasePool {
        pgorm_migration::pgorm::connect(config(&self.base_url, &self.db_name))
    }

    /// Drop the database. A failure is left to the sweep a later run makes once
    /// this process has exited.
    pub async fn delete(&self) {
        if let Err(err) = drop_database(&self.base_url, &self.db_name).await {
            tracing::warn!("test database {:?} outlived its test: {err}", self.db_name);
        }
    }
}

pub async fn has_table(db: &DatabasePool, name: &str) -> Result<bool, Error> {
    let conn = db.get().await?;
    let params: [&(dyn ToSql + Sync); 1] = [&name];
    let row = conn
        .query_one("SELECT to_regclass($1) IS NOT NULL", &params)
        .await?;
    Ok(row.get(0))
}

pub async fn has_index(db: &DatabasePool, table: &str, index: &str) -> Result<bool, Error> {
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

pub async fn has_column(db: &DatabasePool, table: &str, column: &str) -> Result<bool, Error> {
    let conn = db.get().await?;
    let params: [&(dyn ToSql + Sync); 2] = [&table, &column];
    let row = conn
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
             WHERE table_schema = current_schema() AND table_name = $1 AND column_name = $2)",
            &params,
        )
        .await?;
    Ok(row.get(0))
}

pub async fn count_rows(db: &DatabasePool, table: &str) -> Result<i64, Error> {
    let conn = db.get().await?;
    let row = conn
        .query_one(&format!("SELECT COUNT(*) FROM \"{table}\""), &[])
        .await?;
    Ok(row.get(0))
}
