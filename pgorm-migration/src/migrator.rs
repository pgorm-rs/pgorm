use futures::Future;
use std::collections::HashSet;
use std::fmt::Display;
use std::pin::Pin;
use std::time::SystemTime;
use tracing::info;

use super::{seaql_migrations, MigrationTrait};
use pgorm::pgorm_query::{self, IntoIden, Order, Query, QueryBuilder, SelectStatement};
use pgorm::{
    ActiveModelTrait, ActiveValue, ConnectionTrait, DatabasePool, DatabaseTransaction, DbErr,
    DynIden, EntityTrait, FromQueryResult, Iterable, Schema, TransactionTrait,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// Status of migration
pub enum MigrationStatus {
    /// Not yet applied
    Pending,
    /// Applied
    Applied,
}

impl Display for MigrationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = match self {
            MigrationStatus::Pending => "Pending",
            MigrationStatus::Applied => "Applied",
        };
        write!(f, "{status}")
    }
}

pub struct Migration {
    migration: Box<dyn MigrationTrait>,
    status: MigrationStatus,
}

impl Migration {
    /// Get migration name from MigrationName trait implementation
    pub fn name(&self) -> &str {
        self.migration.name()
    }

    /// Get migration status
    pub fn status(&self) -> MigrationStatus {
        self.status
    }
}

/// Performing migrations on a database
#[async_trait::async_trait]
pub trait MigratorTrait: Send {
    /// Vector of migrations in time sequence
    fn migrations() -> Vec<Box<dyn MigrationTrait>>;

    /// Name of the migration table, it is `seaql_migrations` by default
    fn migration_table_name() -> DynIden {
        seaql_migrations::Entity.into_iden()
    }

    /// Get list of migrations wrapped in `Migration` struct
    fn get_migration_files() -> Vec<Migration> {
        Self::migrations()
            .into_iter()
            .map(|migration| Migration {
                migration,
                status: MigrationStatus::Pending,
            })
            .collect()
    }

    /// Get list of applied migrations from database
    async fn get_migration_models(
        db: &(impl ConnectionTrait),
    ) -> Result<Vec<seaql_migrations::Model>, DbErr> {
        Self::install(db).await?;
        let stmt = Query::select()
            .table_name(Self::migration_table_name())
            .columns(seaql_migrations::Column::iter().map(IntoIden::into_iden))
            .order_by(seaql_migrations::Column::Version, Order::Asc)
            .to_owned();
        let (stmt, values) = stmt.build(QueryBuilder);
        seaql_migrations::Model::find_by_statement(stmt, values.0)
            .all(db)
            .await
    }

    /// Get list of migrations with status
    async fn get_migration_with_status(
        db: &(impl ConnectionTrait),
    ) -> Result<Vec<Migration>, DbErr> {
        Self::install(db).await?;
        let mut migration_files = Self::get_migration_files();
        let migration_models = Self::get_migration_models(db).await?;

        let migration_in_db: HashSet<String> = migration_models
            .into_iter()
            .map(|model| model.version)
            .collect();
        let migration_in_fs: HashSet<String> = migration_files
            .iter()
            .map(|file| file.migration.name().to_string())
            .collect();

        let pending_migrations = &migration_in_fs - &migration_in_db;
        for migration_file in migration_files.iter_mut() {
            if !pending_migrations.contains(migration_file.migration.name()) {
                migration_file.status = MigrationStatus::Applied;
            }
        }

        let missing_migrations_in_fs = &migration_in_db - &migration_in_fs;
        let errors: Vec<String> = missing_migrations_in_fs
            .iter()
            .map(|missing_migration| {
                format!("Migration file of version '{missing_migration}' is missing, this migration has been applied but its file is missing")
            }).collect();

        if !errors.is_empty() {
            Err(DbErr::Custom(errors.join("\n")))
        } else {
            Ok(migration_files)
        }
    }

    /// Get list of pending migrations
    async fn get_pending_migrations(db: &(impl ConnectionTrait)) -> Result<Vec<Migration>, DbErr> {
        Self::install(db).await?;
        Ok(Self::get_migration_with_status(db)
            .await?
            .into_iter()
            .filter(|file| file.status == MigrationStatus::Pending)
            .collect())
    }

    /// Get list of applied migrations
    async fn get_applied_migrations(db: &(impl ConnectionTrait)) -> Result<Vec<Migration>, DbErr> {
        Self::install(db).await?;
        Ok(Self::get_migration_with_status(db)
            .await?
            .into_iter()
            .filter(|file| file.status == MigrationStatus::Applied)
            .collect())
    }

    /// Create migration table `seaql_migrations` in the database
    async fn install(db: &(impl ConnectionTrait)) -> Result<(), DbErr> {
        db.execute("CREATE TABLE IF NOT EXISTS seaql_migrations (version TEXT NOT NULL, applied_at BIGINT NOT NULL)", &[]).await?;
        tracing::debug!("Installed");
        Ok(())
    }

    /// Check the status of all migrations
    async fn status(db: &(impl ConnectionTrait)) -> Result<(), DbErr> {
        Self::install(db).await?;

        info!("Checking migration status");

        for Migration { migration, status } in Self::get_migration_with_status(db).await? {
            info!("Migration '{}'... {}", migration.name(), status);
        }

        Ok(())
    }

    /// Apply pending migrations
    async fn up(db: DatabasePool, steps: Option<u32>) -> Result<(), DbErr> {
        tracing::debug!("Applying migrations");
        exec_with_connection::<'_, _>(db, move |manager| {
            tracing::debug!("Exec up");
            Box::pin(async move { exec_up::<Self>(manager, steps).await })
        })
        .await
    }
}

async fn exec_with_connection<'c, F>(db: DatabasePool, f: F) -> Result<(), DbErr>
where
    F: for<'b> Fn(
        &'b DatabaseTransaction<'_>,
    ) -> Pin<Box<dyn Future<Output = Result<(), DbErr>> + Send + 'b>>,
{
    let mut conn = db.get().await?;
    let transaction = conn.begin().await?;
    f(&transaction).await?;
    transaction.commit().await
}

async fn exec_up<M>(db: &DatabaseTransaction<'_>, mut steps: Option<u32>) -> Result<(), DbErr>
where
    M: MigratorTrait + ?Sized,
{
    M::install(db).await?;

    if let Some(steps) = steps {
        info!("Applying {} pending migrations", steps);
    } else {
        info!("Applying all pending migrations");
    }

    let migrations = M::get_pending_migrations(db).await?.into_iter();
    if migrations.len() == 0 {
        info!("No pending migrations");
    }

    for Migration { migration, .. } in migrations {
        if let Some(steps) = steps.as_mut() {
            if steps == &0 {
                break;
            }
            *steps -= 1;
        }
        info!("Applying migration '{}'", migration.name());
        migration.up(db).await?;
        info!("Migration '{}' has been applied", migration.name());
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("SystemTime before UNIX EPOCH!");
        seaql_migrations::Entity::insert(seaql_migrations::ActiveModel {
            version: ActiveValue::Set(migration.name().to_owned()),
            applied_at: ActiveValue::Set(now.as_secs() as i64),
        })
        .table_name(M::migration_table_name())
        .exec(db)
        .await?;
    }

    Ok(())
}

trait QueryTable {
    type Statement;

    fn table_name(self, table_name: DynIden) -> Self::Statement;
}

impl QueryTable for SelectStatement {
    type Statement = SelectStatement;

    fn table_name(mut self, table_name: DynIden) -> SelectStatement {
        self.from(table_name);
        self
    }
}

impl QueryTable for pgorm_query::TableCreateStatement {
    type Statement = pgorm_query::TableCreateStatement;

    fn table_name(mut self, table_name: DynIden) -> pgorm_query::TableCreateStatement {
        self.table(table_name);
        self
    }
}

impl<A> QueryTable for pgorm::Insert<A>
where
    A: ActiveModelTrait,
{
    type Statement = pgorm::Insert<A>;

    fn table_name(mut self, table_name: DynIden) -> pgorm::Insert<A> {
        pgorm::QueryTrait::query(&mut self).into_table(table_name);
        self
    }
}

impl<E> QueryTable for pgorm::DeleteMany<E>
where
    E: EntityTrait,
{
    type Statement = pgorm::DeleteMany<E>;

    fn table_name(mut self, table_name: DynIden) -> pgorm::DeleteMany<E> {
        pgorm::QueryTrait::query(&mut self).from_table(table_name);
        self
    }
}
