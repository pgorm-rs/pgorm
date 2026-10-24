use pgorm_migration::prelude::*;

/// A migration that runs one script, its name pinned by hand.
pub struct Script {
    pub name: &'static str,
    pub sql: &'static str,
}

impl MigrationName for Script {
    fn name(&self) -> &str {
        self.name
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Script {
    async fn up(&self, tx: &DatabaseTransaction<'_>) -> Result<(), Error> {
        tx.batch_execute(self.sql).await
    }
}

const CREATED: Script = Script {
    name: "m20240201_000001_create_quantity",
    sql: "CREATE DOMAIN quantity AS integer DEFAULT 1; \
          CREATE TABLE line (id integer PRIMARY KEY, qty quantity)",
};

const DEFAULTED: Script = Script {
    name: "m20240201_000002_default_quantity",
    sql: "ALTER DOMAIN quantity SET DEFAULT 2",
};

/// A domain with a default, and a table with a column of it.
pub struct Created;

#[async_trait::async_trait]
impl MigratorTrait for Created {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(CREATED)]
    }
}

/// [`Created`], then a migration changing the domain's default.
pub struct Defaulted;

#[async_trait::async_trait]
impl MigratorTrait for Defaulted {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(CREATED), Box::new(DEFAULTED)]
    }
}

const REDEFAULTED: Script = Script {
    name: "m20240201_000003_redefault_quantity",
    sql: "ALTER DOMAIN quantity SET DEFAULT 3",
};

/// [`Defaulted`], then another change to the default, from a migrator that
/// notifies no channel.
pub struct Unannounced;

#[async_trait::async_trait]
impl MigratorTrait for Unannounced {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(CREATED),
            Box::new(DEFAULTED),
            Box::new(REDEFAULTED),
        ]
    }

    fn statement_cache_channel() -> Option<String> {
        None
    }
}
