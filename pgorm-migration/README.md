# pgorm-migration

A minimal, up-only migration runner for pgorm. Migrations are plain Rust —
each one implements `MigrationTrait` with an `up` that receives a connection —
and applied migrations are recorded in a tracking table. There are no down
migrations, no rollback, and no CLI: you write forward migrations and run
them from your own binary.

A starter migration crate lives at
[`template/migration/`](template/migration/) — copy it into your project,
add your migration modules to the migrator, and run it against your
database.

## The ledger

Applied migrations are recorded in a table named `pgorm_migrations`, which by
default sits wherever `search_path` puts an unqualified name — usually beside
your own tables in `public`. A migrator can give it a schema of its own, so
that triggers, policies or grants applied to every application table do not
reach it:

```rust,ignore
#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20220101_000001_create_table::Migration)]
    }

    fn migration_table_schema() -> Option<Name> {
        Some(Name::runtime("pgorm"))
    }
}
```

`up` creates the schema if it is missing, then the ledger inside it, and every
read and write names the table in that schema, so a table of the same name in
`public` is never taken for it. A schema created ahead of time is used as it
is; the runner checks the catalogue first, so a role that owns the schema
needs no `CREATE` privilege on the database. `migration_table_name()` renames
the table the same way.

The runner does not move a ledger that already exists. A ledger left in its
old place reads as absent from the new one, which would run every migration
again, so move it before deploying a release that names the schema:

```sql
CREATE SCHEMA IF NOT EXISTS pgorm;
ALTER TABLE pgorm_migrations SET SCHEMA pgorm;
```

A ledger last written by SeaORM, under `seaql_migrations`, is renamed to
`pgorm_migrations` on the next `up` — but only for the default name with no
schema. A migrator that names or places its own ledger takes it over by hand.

## Schemas

An entity declared with `schema_name` lives in that schema, and so does a
table, type or sequence a migration names as a `(schema, name)` pair. The
schema has to exist first, and a migration creates it with the same builders
as the rest — the prelude brings in pgorm-query's `Schema`:

```rust,ignore
async fn up(&self, tx: &DatabaseTransaction<'_>) -> Result<(), Error> {
    tx.batch_execute(&Schema::create(Name::runtime("billing")).if_not_exists().to_string())
        .await?;
    let invoice = Table::create((Name::runtime("billing"), Name::runtime("invoice")))
        .col(ColumnDef::new(Name::runtime("id")).integer().not_null())
        .to_owned();
    tx.batch_execute(&invoice.to_string()).await?;
    Ok(())
}
```

`Schema::drop(..)` and `Schema::rename(..)` drop and rename one. A drop is
refused while the schema holds anything unless it `cascade()`s, which drops
what it holds too. The prelude's `Schema` is pgorm-query's statement builder;
`pgorm::Schema`, which generates an entity's DDL, is a different type, so a
migration that uses both names that one by its path.

## Domains

A column whose type is a domain needs the domain first, and a migration
creates it with `Domain`, checks written over `Expr::domain_value()`, the
`VALUE` a check tests:

```rust,ignore
async fn up(&self, tx: &DatabaseTransaction<'_>) -> Result<(), Error> {
    let quantity = Domain::create(Name::runtime("quantity"), ColumnType::Integer)
        .default(1)
        .not_null()
        .check(DomainCheck::new(Expr::domain_value().gt(0)).name(Name::runtime("positive")))
        .to_string();
    tx.batch_execute(&quantity).await?;
    Ok(())
}
```

`Domain::alter(..)` takes one action per statement — a default, `NOT NULL`, a
check added (`NOT VALID` or not), validated, renamed or dropped, or the
domain renamed — and `Domain::drop(..)` is refused while a column has the
type unless it `cascade()`s. A changed default reaches only statements
prepared after it: PostgreSQL's plan cache keeps the default an insert was
planned with, and pgorm prepares each statement once per pooled connection.
`up` answers this itself: once a batch that applied a migration commits, it
calls `clear_statement_caches()` on the pool it was given, and each of that
pool's connections prepares its statements again before it is next lent. A
connection the application held checked out across the run keeps its old
statements until it is returned, and another pool on the same database is not
the runner's to clear. DDL applied any other way is followed by the same call:

```rust,ignore
pool.get().await?.batch_execute("ALTER DOMAIN quantity SET DEFAULT 2").await?;
pool.clear_statement_caches();
```
